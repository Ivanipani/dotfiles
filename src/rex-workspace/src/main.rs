mod finder;
mod jj;
mod manifest;
mod ops;
mod repos;
mod rex;
mod tui;

use anstream::{eprintln, println};
use anyhow::{bail, Result};
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};
use comfy_table::{presets, Attribute, Cell, Color, ContentArrangement, Table};
use manifest::{session_label, Env};
use rex::Rex;
use serde_json::json;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use tui::{Action, Launch};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Magenta.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Magenta.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Green.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

/// Workspace sets of jj workspaces, each opened as a Rex session.
///
/// A set lives at ~/.local/share/workspaces/<set>/ with a workspace.yaml
/// manifest and one jj workspace per project. Opening it creates the Rex
/// session `ws-<set>`: a tab at the set root, then per project an nvim | agent
/// split with jjui in a floating layer. The format is shared with
/// zellij-workspace, so a set opens from either.
///
/// With no command, opens the full-screen manager.
#[derive(Parser)]
#[command(
    name = "rex-workspace",
    version,
    styles = STYLES,
    after_long_help = "\
Manager keys (none collide with Rex's ctrl+a, ctrl+h/j/k/l or alt+ chords):
  j/k ↑/↓  move          Enter/o  open        n  new set       a  add repos
  x  remove projects     D  delete set       /  fuzzy filter  Tab  focus projects
  r  reload              ?  help             q/Esc  quit

Environment:
  REX_WORKSPACES_DIR     where sets live (default ~/.local/share/workspaces)
  REX_WORKSPACE_ROOTS    colon-separated roots scanned for repos (default ~/github:~)
  REX_BIN                the rex executable to drive

Examples:
  rex-workspace                       Open the manager
  rex-workspace ls                    Table of sets, projects and sessions
  rex-workspace new feat-x api web    Create a set from two repos and open it
  rex-workspace add feat-x docs       Add a repo; an open session gains a tab
  rex-workspace delete feat-x -y      Delete without asking"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List sets with their projects and session state.
    Ls {
        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Open the manager, optionally straight into an action on a set.
    Tui {
        /// Set to focus (default: this session's set)
        set: Option<String>,
        /// Start by prompting for this action
        #[arg(long, value_parser = ["open", "new", "add", "remove", "delete"])]
        action: Option<String>,
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
        /// Repositories to include, by name (default: pick in the manager)
        repos: Vec<String>,
    },
    /// Add repositories to a set.
    Add {
        /// Set to change (default: this session's set)
        set: Option<String>,
        /// Repositories to add, by name (default: pick in the manager)
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
    let interactive = matches!(cli.command, None | Some(Command::Tui { .. }));
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("\x1b[1;31merror:\x1b[0m {e:#}");
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
    for line in tui::run(env, rex, launch)? {
        println!("{line}");
    }
    Ok(())
}

fn print_notes(notes: Vec<String>) {
    for note in notes {
        println!("{note}");
    }
}

fn run(command: Option<Command>) -> Result<()> {
    let (env, rex) = (Env::from_process(), Rex::locate());
    let Some(command) = command else { return manager(env, rex, Launch::default()) };
    match command {
        Command::Ls { json } => list(&env, &rex, json),
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
            if env.set_dir(&set).exists() {
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
            if !env.set_dir(&set).exists() {
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

fn list(env: &Env, rex: &Rex, json: bool) -> Result<()> {
    let sessions = rex.sessions();
    let rows: Vec<_> = env
        .sets()
        .into_iter()
        .map(|m| {
            let session = sessions.iter().find(|s| s.label == session_label(&m.name));
            let tabs = session.and_then(|s| rex.tab_count(&s.id));
            (m, session.cloned(), tabs)
        })
        .collect();
    if json {
        let out: Vec<_> = rows
            .iter()
            .map(|(m, s, tabs)| json!({"set": m.name, "projects": m.projects, "open": s.is_some(), "session_id": s.as_ref().map(|s| &s.id), "tabs": tabs}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    let mut table = Table::new();
    table.load_style(presets::UTF8_FULL_CONDENSED.with_rounded_corners());
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(["SET", "PROJECTS", "SESSION", "TABS"].map(|t| Cell::new(t).add_attribute(Attribute::Bold).fg(Color::Cyan)));
    for (m, session, tabs) in &rows {
        let names: Vec<_> = m.projects.iter().map(|p| p.name.as_str()).collect();
        let open = if session.is_some() { Cell::new("open").fg(Color::Green) } else { Cell::new("—").add_attribute(Attribute::Dim) };
        let tabs = tabs.map_or(Cell::new("—").add_attribute(Attribute::Dim), Cell::new);
        table.add_row([Cell::new(&m.name), Cell::new(names.join(" ")), open, tabs]);
    }
    if !std::io::stdout().is_terminal() {
        table.force_no_tty();
    }
    println!("{table}");
    Ok(())
}
