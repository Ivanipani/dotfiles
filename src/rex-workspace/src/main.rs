mod jj;
mod manifest;
mod ops;
mod procs;
mod repos;
mod rex;
mod tui;

use anstream::eprintln;
use anyhow::{bail, Result};
use claw::console;
use claw::output::{Format, OutputArgs, Tabular};
use claw::table::{self, Cell};
use clap::{Parser, Subcommand};
use manifest::{session_label, Env};
use rex::Rex;
use serde::Serialize;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use tui::{Action, Launch};

/// Workspace sets of jj workspaces, each opened as a Rex session.
///
/// A set lives at ~/.local/share/workspaces/<set>/ with a workspace.yaml
/// manifest and one jj workspace per project. Opening it creates the Rex
/// session `ws-<set>`: a tab at the set root, then per project a tab with a
/// single nvim pane. The format is shared with
/// zellij-workspace, so a set opens from either.
///
/// With no command, opens the full-screen manager.
#[derive(Parser)]
#[command(
    name = "rex-workspace",
    version,
    styles = claw::cli::STYLES,
    after_long_help = "\
Manager keys (none collide with Rex's ctrl+a, ctrl+h/j/k/l or alt+ chords):
  j/k ↑/↓  move          Enter/o  open        n  new set       a  add repos
  x  remove projects     D  delete set       /  fuzzy filter  Tab  focus projects
  r  reload              ?  help             q/Esc  quit
Procs keys: j/k move  Tab mark  t stop (SIGTERM)  K force kill  / filter  m/c sort by mem/cpu  r refresh  q quit
In pickers: Tab marks, Ctrl-F names an extra workspace (version) of the focused repo.

Environment:
  REX_WORKSPACES_DIR     where sets live (default ~/.local/share/workspaces)
  REX_WORKSPACE_ROOTS    colon-separated roots scanned for repos (default ~/github:~)
  REX_BIN                the rex executable to drive

Examples:
  rex-workspace                       Open the manager
  rex-workspace procs                 Browse and stop processes under your Rex panes
  rex-workspace procs --list          The same as a table
  rex-workspace ls                    Table of sets, projects and sessions
  rex-workspace ls --json             The same as JSON (automatic when piped)
  rex-workspace new feat-x api web    Create a set from two repos and open it
  rex-workspace add feat-x docs       Add a repo; an open session gains a tab
  rex-workspace add feat-x api:v2     Add a second jj workspace of api, as tab api-v2
  rex-workspace delete feat-x -y      Delete without asking"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Subcommand)]
enum Command {
    /// List sets with their projects and session state.
    Ls,
    /// Open the manager, optionally straight into an action on a set.
    Tui {
        /// Set to focus (default: this session's set)
        set: Option<String>,
        /// Start by prompting for this action
        #[arg(long, value_parser = ["open", "new", "add", "remove", "delete"])]
        action: Option<String>,
    },
    /// Browse the processes under your Rex panes, with memory and CPU, and stop them.
    Procs {
        /// Only this Rex session (by label)
        #[arg(short, long)]
        session: Option<String>,
        /// Print a table and exit instead of opening the browser
        #[arg(short, long)]
        list: bool,
    },
    /// Open a set as a Rex session, or switch to it if already open.
    Open {
        /// Set to open (default: pick in the manager)
        set: Option<String>,
    },
    /// Create a set from repositories, then open it.
    New {
        /// Name of the new set
        set: String,
        /// Repositories to include, by name; `repo:feature` adds an extra jj workspace of `repo`
        /// (default: pick in the manager)
        repos: Vec<String>,
    },
    /// Add repositories to a set.
    Add {
        /// Set to change (default: this session's set)
        set: Option<String>,
        /// Repositories to add, by name; `repo:feature` adds an extra jj workspace of `repo`
        /// (default: pick in the manager)
        repos: Vec<String>,
    },
    /// Remove projects from a set.
    Remove {
        /// Set to change (default: this session's set)
        set: Option<String>,
        /// Projects to remove (default: pick in the manager)
        projects: Vec<String>,
    },
    /// Delete a set; uncommitted work is kept under saved-<set> bookmarks.
    Delete {
        /// Set to delete (default: this session's set)
        set: Option<String>,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let interactive = matches!(cli.command, None | Some(Command::Tui { .. }) | Some(Command::Procs { list: false, .. }));
    match run(cli.command, cli.output.format()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            claw::console::error(format!("{e:#}"));
            // A float closes with its process; hold the message long enough to read.
            if interactive && std::io::stdin().is_terminal() {
                eprintln!("press Enter to close");
                let _ = std::io::stdin().read_line(&mut String::new());
            }
            ExitCode::FAILURE
        }
    }
}

fn manager(env: Env, rex: Rex, launch: Launch) -> Result<()> {
    print_notes(tui::run(env, rex, launch)?);
    Ok(())
}

/// What a command did, for people: stderr, so stdout carries only data.
fn print_notes(notes: Vec<String>) {
    for note in notes {
        console::done(note, None);
    }
}

fn run(command: Option<Command>, format: Format) -> Result<()> {
    let (env, rex) = (Env::from_process(), Rex::locate());
    let Some(command) = command else { return manager(env, rex, Launch::default()) };
    match command {
        Command::Ls => list(&env, &rex, format),
        Command::Procs { session, list: true } => list_procs(&rex, session.as_deref(), format),
        Command::Procs { session, list: false } => tui::procs::run(rex, session),
        Command::Tui { set, action } => {
            let action = action.as_deref().map(|a| match a {
                "open" => Action::Open,
                "new" => Action::New,
                "add" => Action::Add,
                "remove" => Action::Remove,
                _ => Action::Delete,
            });
            manager(env, rex, Launch { select: set, action, new_name: None })
        }
        Command::Open { set: None } => manager(env, rex, Launch::default()),
        Command::Open { set: Some(set) } => Ok(print_notes(ops::open(&env, &rex, &set)?)),
        Command::New { set, repos } => {
            if env.exists(&set) {
                bail!("{set} already exists");
            }
            manifest::validate_name(&set)?;
            if repos.is_empty() {
                return manager(env, rex, Launch { action: Some(Action::New), new_name: Some(set), ..Launch::default() });
            }
            let picks = ops::resolve_repos(&env, &set, &repos)?;
            print_notes(ops::add(&env, &rex, &set, &picks)?);
            Ok(print_notes(ops::open(&env, &rex, &set)?))
        }
        Command::Add { set, repos } => match (set.or_else(|| ops::current_set(&rex)), repos.is_empty()) {
            (set, true) => manager(env, rex, Launch { select: set, action: Some(Action::Add), ..Launch::default() }),
            (Some(set), false) => {
                let picks = ops::resolve_repos(&env, &set, &repos)?;
                Ok(print_notes(ops::add(&env, &rex, &set, &picks)?))
            }
            (None, false) => ops::require_current_set(&rex).map(drop),
        },
        Command::Remove { set, projects } => match (set.or_else(|| ops::current_set(&rex)), projects.is_empty()) {
            (set, true) => manager(env, rex, Launch { select: set, action: Some(Action::Remove), ..Launch::default() }),
            (Some(set), false) => Ok(print_notes(ops::remove(&env, &rex, &set, &projects)?)),
            (None, false) => ops::require_current_set(&rex).map(drop),
        },
        Command::Delete { set, yes } => {
            let Some(set) = set.or_else(|| ops::current_set(&rex)) else {
                return manager(env, rex, Launch { action: Some(Action::Delete), ..Launch::default() });
            };
            if !env.exists(&set) {
                bail!("no set named {set}");
            }
            if !yes && !confirm(&format!("Delete workspace set {set}? [y/N] ")) {
                return Ok(());
            }
            Ok(print_notes(ops::delete(&env, &rex, &set)?))
        }
    }
}

fn confirm(prompt: &str) -> bool {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    answer.trim().eq_ignore_ascii_case("y")
}

/// One `ls` row: a set, its projects and its Rex session.
#[derive(Serialize)]
struct Listing {
    set: String,
    projects: Vec<manifest::Project>,
    open: bool,
    session_id: Option<String>,
    tabs: Option<usize>,
}

impl Tabular for Listing {
    const COLUMNS: &'static [&'static str] = &["SET", "PROJECTS", "SESSION", "TABS"];

    fn row(&self) -> Vec<Cell> {
        let names: Vec<_> = self.projects.iter().map(|p| p.name.as_str()).collect();
        let open = if self.open { table::ok("open") } else { table::optional(None::<&str>) };
        vec![Cell::new(&self.set), Cell::new(names.join(" ")), open, table::optional(self.tabs)]
    }
}

fn list(env: &Env, rex: &Rex, format: Format) -> Result<()> {
    let sessions = rex.sessions();
    let rows: Vec<_> = env
        .sets()
        .into_iter()
        .map(|m| {
            let session = sessions.iter().find(|s| s.label == session_label(&m.name));
            Listing {
                tabs: session.and_then(|s| rex.tab_count(&s.id)),
                open: session.is_some(),
                session_id: session.map(|s| s.id.clone()),
                set: m.name,
                projects: m.projects,
            }
        })
        .collect();
    format.list(&rows, "no workspace sets yet; create one with rex-workspace new");
    Ok(())
}

fn list_procs(rex: &Rex, session: Option<&str>, format: Format) -> Result<()> {
    let snap = procs::collect(rex, session)?;
    format.list(&snap.rows, "no processes under your Rex panes");
    if !format.is_json() {
        for line in procs::summary(procs::Usage::of(snap.rows.iter().map(|r| &r.proc)), snap.all, snap.host.as_ref()) {
            console::note(format!("rex {line}"));
        }
    }
    Ok(())
}
