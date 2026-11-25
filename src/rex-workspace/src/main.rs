mod jj;
mod manifest;
mod ops;
mod park;
mod procs;
mod repos;
mod rex;
mod tui;

use anstream::eprintln;
use anyhow::{bail, Result};
use claw::console;
use claw::output::{Format, OutputArgs, Tabular};
use claw::table::{self, Cell};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use clap_complete::CompleteEnv;
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
  x  remove projects     D  delete set       c  close set     p  pause/resume
  s  sync with trunk     e  edit goal        /  fuzzy filter  Tab  focus projects
  r  reload              ?  help             q/Esc  quit
Procs keys: j/k move  Tab mark  t stop (SIGTERM)  K force kill  / filter  m/c sort by mem/cpu  r refresh  q quit
In pickers: Tab marks, Ctrl-F names an extra workspace (version) of the focused repo.

Environment:
  REX_WORKSPACES_DIR     where sets live (default ~/.local/share/workspaces)
  REX_WORKSPACE_ROOTS    colon-separated roots scanned for repos (default ~/github:~)
  REX_BIN                the rex executable to drive

Shell completion (sets and repositories by name), e.g. in ~/.zshrc:
  source <(COMPLETE=zsh rex-workspace)

Examples:
  rex-workspace                       Open the manager
  rex-workspace procs                 Browse and stop processes under your Rex panes
  rex-workspace procs --list          The same as a table
  rex-workspace ls                    Table of sets, projects and sessions
  rex-workspace ls --json             The same as JSON (automatic when piped)
  rex-workspace new feat-x api web    Create a set from two repos and open it
  rex-workspace new feat-x api -m 'Add retries to uploads' --link https://…/PR-12
                                      … stating its goal in the set's AGENTS.md
  rex-workspace describe feat-x       Print the set's goal and links
  rex-workspace describe -m 'New goal'  Change this session's set's goal
  rex-workspace sync feat-x           Fetch, then rebase every project onto trunk()
  rex-workspace stranded              Work in this set that exists only on this machine
  rex-workspace reap                  Agent bookmarks (landed, active, stale) and orphaned commits
  rex-workspace reap --prune          … deleting the landed bookmarks
  rex-workspace tidy --orphans        Delete this repo's landed bookmarks, abandon orphaned work
  rex-workspace add feat-x docs       Add a repo; an open session gains a tab
  rex-workspace add feat-x api:v2     Add a second jj workspace of api, as tab api-v2
  rex-workspace close feat-x          End the session, freeing everything; open brings its tabs back
  rex-workspace pause feat-x          Stop its processes (no CPU, memory kept); resume or open continues
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
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
        /// Start by prompting for this action
        #[arg(long, value_parser = ["open", "new", "add", "remove", "delete", "close", "pause"])]
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
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
    },
    /// Create a set from repositories, then open it.
    New {
        /// Name of the new set
        set: String,
        /// Repositories to include, by name; `repo:feature` adds an extra jj workspace of `repo`
        /// (default: pick in the manager)
        #[arg(add = ArgValueCandidates::new(repo_names))]
        repos: Vec<String>,
        /// What the set is for, written into its AGENTS.md
        #[arg(short = 'm', long, requires = "repos")]
        goal: Option<String>,
        /// A ticket, PR or doc about the goal (repeatable)
        #[arg(short, long, requires = "repos")]
        link: Vec<String>,
    },
    /// Add repositories to a set.
    Add {
        /// Set to change (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
        /// Repositories to add, by name; `repo:feature` adds an extra jj workspace of `repo`
        /// (default: pick in the manager)
        #[arg(add = ArgValueCandidates::new(repo_names))]
        repos: Vec<String>,
    },
    /// Remove projects from a set.
    Remove {
        /// Set to change (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
        /// Projects to remove (default: pick in the manager)
        projects: Vec<String>,
    },
    /// Save a set's layout and end its session, freeing every process; `open` restores it.
    Close {
        /// Set to close (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
    },
    /// Stop every process in a set's session: no CPU, memory kept.
    Pause {
        /// Set to pause (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
    },
    /// Continue a paused set's processes without switching to it.
    Resume {
        /// Set to resume (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
    },
    /// Fetch each repository, then rebase every project's working copy onto trunk().
    ///
    /// The branch of `@` (every commit not yet on trunk) moves onto trunk() when trunk has new
    /// commits; conflicts are reported, not resolved. A repository without a remote main, master
    /// or trunk is skipped.
    Sync {
        /// Set to sync (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
    },
    /// Show or change what a set is for; its AGENTS.md states it for agents.
    ///
    /// With no options, prints the goal and links.
    Describe {
        /// Set to describe (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
        /// The new goal; an empty one clears it
        #[arg(short = 'm', long)]
        goal: Option<String>,
        /// Add a ticket, PR or doc about the goal (repeatable)
        #[arg(short, long)]
        link: Vec<String>,
        /// Drop the existing links first
        #[arg(long)]
        clear_links: bool,
    },
    /// List work in a set's workspaces that exists only on this machine.
    ///
    /// Each row is the tip of a run of commits on the branch of `@` that no remote bookmark holds.
    /// Push it under the workspace's own prefix with `jj git push --named <prefix><name>=<change>`, or abandon it.
    /// Projects without a remote are left out.
    Stranded {
        /// Set to check (default: the one this runs in)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
    },
    /// List agent bookmarks and orphaned commits across every set's repositories.
    ///
    /// Fetches each repository, then lists every `agent/…` bookmark on a remote as landed (already
    /// on trunk), active (owned by a set here, or changed recently) or stale, plus orphans: commits
    /// that no bookmark and no workspace points at. Nothing is deleted unless --prune is given, and
    /// then only landed bookmarks.
    Reap {
        /// Days without change before an unowned bookmark counts as stale
        #[arg(long, default_value_t = 14)]
        days: i64,
        /// Delete landed agent bookmarks, here and on their remotes
        #[arg(long)]
        prune: bool,
    },
    /// Delete the landed bookmarks of the repository here, locally and on their remotes.
    ///
    /// Fetches first. A bookmark is landed when trunk contains its target here and on every remote
    /// (a remote's older version of a change that landed counts). Long-lived bookmarks are kept:
    /// main, master, trunk, develop, dev, prod, production, staging, release/*, or the globs of
    /// jj's `flight.keep-bookmarks` setting. Lists what would go and asks first. `jj tidy` runs it.
    Tidy {
        /// Also abandon orphaned work: commits no bookmark, remote or workspace points at
        #[arg(long)]
        orphans: bool,
        /// Only list what would be cleared
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Delete a set; work that exists only here is pushed under agent/<workspace>/saved bookmarks.
    Delete {
        /// Set to delete (default: this session's set)
        #[arg(add = ArgValueCandidates::new(set_names))]
        set: Option<String>,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    CompleteEnv::with_factory(Cli::command).complete();
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
                "close" => Action::Close,
                "pause" => Action::Pause,
                _ => Action::Delete,
            });
            manager(env, rex, Launch { select: set, action, new_name: None })
        }
        Command::Open { set: None } => manager(env, rex, Launch::default()),
        Command::Open { set: Some(set) } => Ok(print_notes(ops::open(&env, &rex, &set)?)),
        Command::New { set, repos, goal, link } => {
            if env.exists(&set) {
                bail!("{set} already exists");
            }
            manifest::validate_name(&set)?;
            if repos.is_empty() {
                return manager(env, rex, Launch { action: Some(Action::New), new_name: Some(set), ..Launch::default() });
            }
            let picks = ops::resolve_repos(&env, &set, &repos)?;
            print_notes(ops::add(&env, &rex, &set, &picks)?);
            if goal.is_some() || !link.is_empty() {
                print_notes(ops::describe(&env, &set, goal.as_deref(), &link, false)?);
            }
            Ok(print_notes(ops::open(&env, &rex, &set)?))
        }
        Command::Add { set, repos } => match (set.or_else(|| ops::current_set(&env, &rex)), repos.is_empty()) {
            (set, true) => manager(env, rex, Launch { select: set, action: Some(Action::Add), ..Launch::default() }),
            (Some(set), false) => {
                let picks = ops::resolve_repos(&env, &set, &repos)?;
                Ok(print_notes(ops::add(&env, &rex, &set, &picks)?))
            }
            (None, false) => ops::require_current_set(&env, &rex).map(drop),
        },
        Command::Remove { set, projects } => match (set.or_else(|| ops::current_set(&env, &rex)), projects.is_empty()) {
            (set, true) => manager(env, rex, Launch { select: set, action: Some(Action::Remove), ..Launch::default() }),
            (Some(set), false) => Ok(print_notes(ops::remove(&env, &rex, &set, &projects)?)),
            (None, false) => ops::require_current_set(&env, &rex).map(drop),
        },
        Command::Close { set } => Ok(print_notes(ops::close(&env, &rex, &named_or_current(&env, &rex, set)?)?)),
        Command::Pause { set } => Ok(print_notes(ops::pause(&env, &rex, &named_or_current(&env, &rex, set)?)?)),
        Command::Resume { set } => Ok(print_notes(ops::resume(&env, &rex, &named_or_current(&env, &rex, set)?)?)),
        Command::Sync { set } => sync(&env, &named_or_current(&env, &rex, set)?),
        Command::Stranded { set } => {
            let set = named_or_current(&env, &rex, set)?;
            format.list(&ops::stranded(&env, &set)?, &format!("nothing in {set} exists only on this machine"));
            Ok(())
        }
        Command::Reap { days, prune } => {
            let (rows, notes) = ops::reap(&env, days, prune)?;
            print_notes(notes);
            format.list(&rows, "no agent bookmarks or orphaned commits");
            Ok(())
        }
        Command::Describe { set, goal, link, clear_links } => {
            let set = named_or_current(&env, &rex, set)?;
            if goal.is_none() && link.is_empty() && !clear_links {
                return show_goal(&env, &set, format);
            }
            Ok(print_notes(ops::describe(&env, &set, goal.as_deref(), &link, clear_links)?))
        }
        Command::Tidy { orphans, dry_run, yes } => tidy(orphans, dry_run, yes, format),
        Command::Delete { set, yes } => {
            let Some(set) = set.or_else(|| ops::current_set(&env, &rex)) else {
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

fn sync(env: &Env, set: &str) -> Result<()> {
    console::start(format!("syncing {set}"));
    let reports = ops::sync(env, set)?;
    for r in &reports {
        if r.ok() {
            console::done(r.summary(), None);
        } else {
            console::error(r.summary());
        }
    }
    let failed = reports.iter().filter(|r| !r.ok()).count();
    if failed > 0 {
        bail!("{failed} of {} project(s) need attention", reports.len());
    }
    Ok(())
}

#[derive(Serialize)]
struct Goal {
    set: String,
    goal: Option<String>,
    links: Vec<String>,
}

fn show_goal(env: &Env, set: &str, format: Format) -> Result<()> {
    if !env.exists(set) {
        bail!("no set named {set}");
    }
    let m = env.load(set)?;
    let goal = Goal { set: m.name, goal: m.goal, links: m.links };
    if format.is_json() {
        println!("{}", serde_json::to_string_pretty(&goal)?);
        return Ok(());
    }
    match &goal.goal {
        Some(text) => println!("{text}"),
        None => console::note(format!("{set} has no goal; set one with rex-workspace describe {set} -m '…'")),
    }
    for link in &goal.links {
        println!("{link}");
    }
    Ok(())
}

/// Completion candidates: every set, described by its goal.
fn set_names() -> Vec<CompletionCandidate> {
    Env::from_process()
        .sets()
        .into_iter()
        .map(|m| {
            let help = m.goal.or_else(|| Some(m.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(" ")));
            CompletionCandidate::new(m.name).help(help.map(Into::into))
        })
        .collect()
}

/// Completion candidates: every repository under the roots, described by its path.
fn repo_names() -> Vec<CompletionCandidate> {
    repos::scan(&Env::from_process().roots)
        .into_iter()
        .map(|p| CompletionCandidate::new(p.name).help(p.source.map(|s| tui::tilde(&s).into())))
        .collect()
}

fn tidy(orphans: bool, dry_run: bool, yes: bool, format: Format) -> Result<()> {
    let dir = jj::root(&std::env::current_dir()?)?;
    let (plan, notes) = ops::tidy_plan(&dir, orphans)?;
    print_notes(notes);
    if format.is_json() {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        for l in &plan.landed {
            println!("landed  {}{}", l.name, l.remotes.iter().map(|r| format!(" @{r}")).collect::<String>());
        }
        for o in &plan.orphans {
            println!("orphan  {o}");
        }
    }
    if plan.is_empty() {
        console::note("nothing to tidy");
        return Ok(());
    }
    if dry_run || (!yes && !confirm("Delete these bookmarks (and abandon the orphans)? [y/N] ")) {
        return Ok(());
    }
    Ok(print_notes(ops::tidy(&dir, &plan)))
}

fn named_or_current(env: &Env, rex: &Rex, set: Option<String>) -> Result<String> {
    set.map_or_else(|| ops::require_current_set(env, rex), Ok)
}

fn confirm(prompt: &str) -> bool {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    answer.trim().eq_ignore_ascii_case("y")
}

impl Tabular for ops::StrandedRow {
    const COLUMNS: &'static [&'static str] = &["PROJECT", "CHANGE", "COMMITS", "BOOKMARKS", "DESCRIPTION"];

    fn row(&self) -> Vec<Cell> {
        let h = &self.head;
        let description = Some(h.description.as_str()).filter(|d| !d.is_empty());
        vec![
            Cell::new(&self.project),
            Cell::new(&h.change),
            Cell::new(h.commits),
            table::optional(Some(h.bookmarks.join(" ")).filter(|b| !b.is_empty())),
            table::optional(description),
        ]
    }
}

impl Tabular for ops::ReapRow {
    const COLUMNS: &'static [&'static str] = &["REPO", "STATE", "BOOKMARK", "CHANGE", "AGE", "DESCRIPTION"];

    fn row(&self) -> Vec<Cell> {
        let state = match self.state {
            ops::Leftover::Landed => table::ok("landed"),
            ops::Leftover::Active => Cell::new("active"),
            ops::Leftover::Stale => table::dim("stale"),
            ops::Leftover::Orphan => table::dim("orphan"),
        };
        vec![
            Cell::new(&self.repo),
            state,
            table::optional(self.bookmark.as_deref()),
            Cell::new(&self.change),
            Cell::new(format!("{}d", self.age_days)),
            table::optional(Some(self.description.as_str()).filter(|d| !d.is_empty())),
        ]
    }
}

/// One `ls` row: a set, its projects and its Rex session.
#[derive(Serialize)]
struct Listing {
    set: String,
    projects: Vec<manifest::Project>,
    open: bool,
    /// open, paused, closed (layout saved, no session) or empty.
    state: &'static str,
    session_id: Option<String>,
    tabs: Option<usize>,
}

impl Tabular for Listing {
    const COLUMNS: &'static [&'static str] = &["SET", "PROJECTS", "SESSION", "TABS"];

    fn row(&self) -> Vec<Cell> {
        let names: Vec<_> = self.projects.iter().map(|p| p.name.as_str()).collect();
        let open = match self.state {
            "open" => table::ok("open"),
            "" => table::optional(None::<&str>),
            other => table::dim(other),
        };
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
                state: park::state(env, &m.name, session.is_some()).label(),
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
