mod config;
mod layout;
mod repos;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use zellij_tile::prelude::*;

use config::Config;
use layout::{compose, Project, DEFAULT_TEMPLATE};
use repos::{list_workspaces, members, sanitize_name, scan_repos, workspace_path, Repo};
use zellij_common::host::{mounted_path, HOST_MOUNT};
use zellij_common::list::{move_cursor, scroll_window};
use zellij_common::ui::{render_row, BOLD, DIM, GREEN, ORANGE, RED, RESET};
use zellij_common::fuzzy;

/// `$1` source repository, `$2` workspace name, `$3` destination.
const ADD_SCRIPT: &str = r#"mkdir -p "$(dirname "$3")" && jj -R "$1" workspace add --name "$2" "$3""#;
/// `$1` is `one` (`$2` a workspace directory) or `set` (`$2` a set directory
/// of them). A workspace with uncommitted changes is first kept alive by a
/// `saved-<set>` bookmark (suffixed `-2`, `-3`, … if taken); if that or the
/// forget fails, nothing is deleted. Prints `saved <bookmark>` for each.
const WORKSPACE_SCRIPT: &str = r#"
forget() {
  d=$1
  if [ "$(jj -R "$d" log -r @ --no-graph -T empty)" = false ]; then
    base=saved-$(basename "$(dirname "$d")"); name=$base; n=1
    while [ -n "$(jj -R "$d" bookmark list -T 'name ++ "\n"' "exact:$name")" ]; do
      n=$((n+1)); name=$base-$n
    done
    jj -R "$d" bookmark create "$name" -r @ >&2 || return 1
    echo "saved $name in $(basename "$d")"
  fi
  jj -R "$d" workspace forget >&2
}
case $1 in
  one) forget "$2" && rm -rf "$2" ;;
  set) for d in "$2"/*/; do [ -d "$d" ] || continue; forget "$d" || exit 1; done; rm -rf "$2" ;;
esac
"#;
/// `$1` the session name. Detached and delayed so it outlives, and runs after
/// leaving, the session it removes.
const KILL_SESSION_SCRIPT: &str = r#"(sleep 1; zellij delete-session --force "$1") >/dev/null 2>&1 &"#;

#[derive(Default, PartialEq)]
enum Mode {
    #[default]
    Home,
    Name,
    Repos,
    Confirm,
}

#[derive(Default)]
struct State {
    config: Config,
    mode: Mode,

    /// Existing workspace sets. The Home list is "new" followed by these.
    workspaces: Vec<String>,
    home_cursor: usize,
    name_input: String,
    /// The workspace set awaiting delete confirmation.
    doomed: String,
    /// Bookmarks created for unsaved changes by the last delete.
    notes: Vec<String>,

    /// The workspace set being created or edited.
    target: String,
    is_new: bool,
    repos: Vec<Repo>,
    initial: BTreeSet<usize>,
    selected: BTreeSet<usize>,
    query: String,
    matches: Vec<fuzzy::Match>,
    cursor: usize,

    live_sessions: Vec<String>,
    current_session: Option<String>,
    tabs: Vec<TabInfo>,

    template: String,
    pending: usize,
    failures: Vec<String>,

    permissions_granted: bool,
    host_ready: bool,
    error: Option<String>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.config = Config::from_map(&configuration);

        set_selectable(true);
        subscribe(&[
            EventType::Key,
            EventType::PermissionRequestResult,
            EventType::Visible,
            EventType::HostFolderChanged,
            EventType::FailedToChangeHostFolder,
            EventType::TabUpdate,
            EventType::RunCommandResult,
        ]);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::FullHdAccess,
            PermissionType::RunCommands,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(status) => {
                self.permissions_granted = matches!(status, PermissionStatus::Granted);
                if self.permissions_granted {
                    change_host_folder(PathBuf::from("/"));
                } else {
                    self.error = Some("Permissions denied — cannot manage workspaces.".to_string());
                }
                true
            }
            Event::HostFolderChanged(path) if path == Path::new("/") => {
                self.host_ready = true;
                self.reload_home();
                true
            }
            Event::FailedToChangeHostFolder(error) => {
                self.error = Some(error.unwrap_or_else(|| "Cannot access host filesystem".into()));
                true
            }
            Event::Visible(true) if self.host_ready && self.mode == Mode::Home => {
                self.reload_home();
                true
            }
            Event::TabUpdate(tabs) => {
                self.tabs = tabs;
                false
            }
            Event::RunCommandResult(code, stdout, stderr, context) => self.job_finished(code, &stdout, &stderr, &context),
            Event::Key(key) if self.host_ready || matches!(key.bare_key, BareKey::Esc) => self.handle_key(key),
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        match self.mode {
            Mode::Home => self.render_home(rows, cols),
            Mode::Name => self.render_name(),
            Mode::Repos => self.render_repos(rows, cols),
            Mode::Confirm => self.render_confirm(),
        }
    }
}

impl State {
    fn mount() -> &'static Path {
        Path::new(HOST_MOUNT)
    }

    fn busy(&self) -> bool {
        self.pending > 0
    }

    /// Refreshes everything the Home list shows.
    fn reload_home(&mut self) {
        self.workspaces = list_workspaces(&self.config, Self::mount());
        if let Ok(snapshot) = get_session_list() {
            self.current_session =
                snapshot.live_sessions.iter().find(|s| s.is_current_session).map(|s| s.name.clone());
            self.live_sessions = snapshot.live_sessions.into_iter().map(|s| s.name).collect();
        }
        // Start on the workspace this session belongs to, else on "new".
        let current = self.workspaces.iter().position(|w| Some(w) == self.current_session.as_ref());
        self.home_cursor = current.map_or(0, |i| i + 1);
    }

    fn is_live(&self, name: &str) -> bool {
        self.live_sessions.iter().any(|s| s == name)
    }

    /// Whether tab changes for `target` should be made in this very session.
    fn syncs_tabs(&self) -> bool {
        self.current_session.as_deref() == Some(self.target.as_str())
    }

    fn enter_repos(&mut self, target: String, is_new: bool) {
        let mut repos = scan_repos(&self.config, Self::mount());
        let present = members(&self.config, Self::mount(), &target);
        for name in &present {
            if !repos.iter().any(|r| &r.name == name) {
                repos.push(Repo { name: name.clone(), source: None });
            }
        }
        self.initial = repos.iter().enumerate().filter(|(_, r)| present.contains(&r.name)).map(|(i, _)| i).collect();
        self.selected = self.initial.clone();
        self.repos = repos;
        self.target = target;
        self.is_new = is_new;
        self.query.clear();
        self.error = None;
        self.failures.clear();
        self.refilter();
        self.mode = Mode::Repos;
    }

    fn refilter(&mut self) {
        let haystacks: Vec<String> = self.repos.iter().map(Repo::label).collect();
        self.matches = fuzzy::filter_sort(&haystacks, &self.query);
        self.cursor = 0;
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if self.busy() {
            return false;
        }
        let ctrl = key.key_modifiers.contains(&KeyModifier::Ctrl);
        if ctrl && key.bare_key == BareKey::Char('c') {
            close_self();
            return true;
        }
        match self.mode {
            Mode::Home => self.handle_home_key(key.bare_key, ctrl),
            Mode::Name => self.handle_name_key(key.bare_key, ctrl),
            Mode::Repos => self.handle_repos_key(key.bare_key, ctrl),
            Mode::Confirm => self.handle_confirm_key(key.bare_key),
        }
    }

    fn handle_home_key(&mut self, key: BareKey, ctrl: bool) -> bool {
        let len = self.workspaces.len() + 1;
        match key {
            BareKey::Esc => {
                close_self();
                true
            }
            BareKey::Down => move_cursor(&mut self.home_cursor, len, 1),
            BareKey::Up => move_cursor(&mut self.home_cursor, len, -1),
            BareKey::Char('j') => move_cursor(&mut self.home_cursor, len, 1),
            BareKey::Char('k') => move_cursor(&mut self.home_cursor, len, -1),
            BareKey::Char('r') if ctrl => {
                self.reload_home();
                true
            }
            BareKey::Char('d') => match self.home_cursor.checked_sub(1) {
                Some(i) => {
                    self.notes.clear();
                    self.doomed = self.workspaces[i].clone();
                    self.error = None;
                    self.mode = Mode::Confirm;
                    true
                }
                None => false,
            },
            BareKey::Enter => {
                self.error = None;
                match self.home_cursor.checked_sub(1) {
                    None => {
                        self.name_input.clear();
                        self.mode = Mode::Name;
                    }
                    Some(i) => {
                        let name = self.workspaces[i].clone();
                        if self.is_live(&name) && self.current_session.as_deref() != Some(name.as_str()) {
                            switch_session(Some(&name));
                            close_self();
                        } else {
                            self.enter_repos(name, false);
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn handle_confirm_key(&mut self, key: BareKey) -> bool {
        match key {
            BareKey::Char('y') => {
                let dir = workspace_path(&self.config, &self.doomed, None);
                let context = BTreeMap::from([("op".to_string(), "delete".to_string())]);
                run_command(&["sh", "-c", WORKSPACE_SCRIPT, "sh", "set", &dir.to_string_lossy()], context);
                self.pending = 1;
                true
            }
            BareKey::Char('n') | BareKey::Esc => {
                self.mode = Mode::Home;
                true
            }
            _ => false,
        }
    }

    /// The set's directories are gone. Leave its session if we are in it, then
    /// have it deleted (killed, if running) from outside.
    fn set_deleted(&mut self) {
        let doomed = self.doomed.clone();
        if self.is_live(&doomed) || self.current_session.as_deref() == Some(doomed.as_str()) {
            run_command(&["sh", "-c", KILL_SESSION_SCRIPT, "sh", &doomed], BTreeMap::new());
        }
        if self.current_session.as_deref() == Some(doomed.as_str()) {
            let other = self.live_sessions.iter().find(|s| **s != doomed).cloned();
            switch_session(other.as_deref());
            close_self();
        } else {
            self.reload_home();
            self.mode = Mode::Home;
        }
    }

    fn handle_name_key(&mut self, key: BareKey, ctrl: bool) -> bool {
        match key {
            BareKey::Esc => {
                self.mode = Mode::Home;
                self.error = None;
                true
            }
            BareKey::Backspace => {
                self.name_input.pop();
                true
            }
            BareKey::Enter => {
                let name = sanitize_name(&self.name_input);
                if name.is_empty() {
                    self.error = Some("Enter a name.".into());
                } else if self.workspaces.contains(&name) {
                    self.error = Some(format!("'{name}' already exists — pick it from the list to edit it."));
                } else {
                    self.enter_repos(name, true);
                }
                true
            }
            BareKey::Char(c) if !ctrl => {
                self.name_input.push(c);
                self.error = None;
                true
            }
            _ => false,
        }
    }

    fn handle_repos_key(&mut self, key: BareKey, ctrl: bool) -> bool {
        match key {
            BareKey::Esc => {
                self.mode = Mode::Home;
                self.error = None;
                self.failures.clear();
                true
            }
            BareKey::Down => move_cursor(&mut self.cursor, self.matches.len(), 1),
            BareKey::Up => move_cursor(&mut self.cursor, self.matches.len(), -1),
            BareKey::Char('j') if ctrl => move_cursor(&mut self.cursor, self.matches.len(), 1),
            BareKey::Char('k') if ctrl => move_cursor(&mut self.cursor, self.matches.len(), -1),
            BareKey::Char('u') if ctrl => {
                self.query.clear();
                self.refilter();
                true
            }
            BareKey::Char('r') if ctrl => {
                let (target, is_new) = (self.target.clone(), self.is_new);
                self.enter_repos(target, is_new);
                true
            }
            BareKey::Tab => self.toggle_current(),
            BareKey::Backspace => {
                self.query.pop();
                self.refilter();
                true
            }
            BareKey::Enter => self.apply(),
            BareKey::Char(c) if !ctrl => {
                self.query.push(c);
                self.refilter();
                true
            }
            _ => false,
        }
    }

    fn toggle_current(&mut self) -> bool {
        let Some(m) = self.matches.get(self.cursor) else { return false };
        if !self.selected.remove(&m.index) {
            self.selected.insert(m.index);
        }
        self.error = None;
        true
    }

    fn project_dir(&self, name: &str) -> PathBuf {
        workspace_path(&self.config, &self.target, Some(name))
    }

    fn read_template(&self) -> Result<String, String> {
        if self.config.layout.is_empty() {
            return Ok(DEFAULT_TEMPLATE.to_string());
        }
        mounted_path(Self::mount(), Path::new(&self.config.layout))
            .and_then(std::fs::read_to_string)
            .map_err(|e| format!("cannot read layout {}: {e}", self.config.layout))
    }

    /// Runs the difference between what was on disk and what is now selected:
    /// adds become `jj workspace add` (plus a tab), removals close the tab,
    /// forget the workspace and delete its directory.
    fn apply(&mut self) -> bool {
        if self.selected.is_empty() {
            self.error = Some("Select at least one project.".into());
            return true;
        }
        self.template = match self.read_template() {
            Ok(template) => template,
            Err(err) => {
                self.error = Some(err);
                return true;
            }
        };
        self.failures.clear();
        self.error = None;

        let sync = self.syncs_tabs();
        let mut jobs: Vec<(Vec<String>, BTreeMap<String, String>)> = Vec::new();
        let job_context = |op: &str, project: &str| {
            BTreeMap::from([("op".to_string(), op.to_string()), ("project".to_string(), project.to_string())])
        };

        for &i in self.selected.difference(&self.initial) {
            let repo = &self.repos[i];
            let Some(source) = &repo.source else { continue };
            let args = ["sh", "-c", ADD_SCRIPT, "sh"].map(String::from).into_iter().chain([
                source.to_string_lossy().into_owned(),
                self.target.clone(),
                self.project_dir(&repo.name).to_string_lossy().into_owned(),
            ]);
            jobs.push((args.collect(), job_context("add", &repo.name)));
        }
        for &i in self.initial.difference(&self.selected) {
            let repo = &self.repos[i];
            if sync {
                self.tabs.iter().filter(|t| t.name == repo.name).for_each(|t| close_tab_with_id(t.tab_id as u64));
            }
            let args = ["sh", "-c", WORKSPACE_SCRIPT, "sh", "one"].map(String::from).into_iter().chain([
                self.project_dir(&repo.name).to_string_lossy().into_owned(),
            ]);
            jobs.push((args.collect(), job_context("remove", &repo.name)));
        }

        self.pending = jobs.len();
        for (args, context) in jobs {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            run_command(&args, context);
        }
        if self.pending == 0 {
            self.finish();
        }
        true
    }

    fn job_finished(&mut self, code: Option<i32>, stdout: &[u8], stderr: &[u8], context: &BTreeMap<String, String>) -> bool {
        if context.get("op").map(String::as_str) == Some("delete") {
            self.pending = 0;
            if code == Some(0) {
                self.notes = String::from_utf8_lossy(stdout).lines().map(str::to_string).collect();
                self.set_deleted();
            } else {
                let detail = String::from_utf8_lossy(stderr);
                self.error = Some(format!("delete {}: {}", self.doomed, detail.trim()));
                self.mode = Mode::Home;
            }
            return true;
        }
        let (Some(op), Some(project)) = (context.get("op"), context.get("project")) else { return false };
        if code == Some(0) {
            if op == "add" && self.syncs_tabs() {
                let dir = self.project_dir(project);
                match compose(&self.template, &[Project { name: project.clone(), dir }]) {
                    Ok(kdl) => {
                        new_tabs_with_layout(&kdl);
                    }
                    Err(err) => self.failures.push(err),
                }
            }
        } else {
            let detail = String::from_utf8_lossy(stderr);
            self.failures.push(format!("{op} {project}: {}", detail.trim()));
        }
        self.pending = self.pending.saturating_sub(1);
        if self.pending == 0 {
            self.finish();
        }
        true
    }

    /// All jobs are done: leave, or stay to show what failed. A set that is new
    /// (or not running) also gets its session, with a tab per member.
    fn finish(&mut self) {
        if !self.failures.is_empty() {
            return;
        }
        if !self.syncs_tabs() {
            let projects: Vec<Project> = members(&self.config, Self::mount(), &self.target)
                .into_iter()
                .map(|name| Project { dir: self.project_dir(&name), name })
                .collect();
            match compose(&self.template, &projects) {
                Ok(kdl) => switch_session_with_layout(
                    Some(&self.target),
                    LayoutInfo::Stringified(kdl),
                    Some(workspace_path(&self.config, &self.target, None)),
                ),
                Err(err) => {
                    self.failures.push(err);
                    return;
                }
            }
        }
        close_self();
    }

    fn render_status(&self) -> bool {
        if let Some(err) = &self.error {
            println!("{RED}{err}{RESET}");
        }
        if !self.permissions_granted {
            println!("{DIM}waiting for permissions…{RESET}");
            return true;
        }
        if !self.host_ready {
            println!("{DIM}waiting for host filesystem…{RESET}");
            return true;
        }
        false
    }

    fn render_home(&self, rows: usize, cols: usize) {
        println!("{BOLD}Workspaces{RESET} — ↑/↓ move, ⏎ open, d delete, esc quit");
        println!();
        for note in &self.notes {
            println!("{GREEN}{note}{RESET}");
        }
        if self.render_status() {
            return;
        }
        let entries = std::iter::once("＋ new workspace".to_string()).chain(self.workspaces.iter().cloned());
        let entries: Vec<String> = entries.collect();
        let window = scroll_window(self.home_cursor, entries.len(), rows.saturating_sub(3 + self.notes.len()));
        for i in window {
            let (glyph, color) = match i.checked_sub(1).map(|w| &self.workspaces[w]) {
                None => ("  ", DIM),
                Some(name) if self.current_session.as_deref() == Some(name.as_str()) => ("◉ ", GREEN),
                Some(name) if self.is_live(name) => ("● ", ORANGE),
                Some(_) => ("  ", DIM),
            };
            println!("{}", render_row(glyph, color, &entries[i], &[], i == self.home_cursor, cols));
        }
    }

    fn render_confirm(&self) {
        let count = members(&self.config, Self::mount(), &self.doomed).len();
        println!("{BOLD}Delete '{}'?{RESET}", self.doomed);
        println!();
        println!("Forgets {count} jj workspace(s) and removes their directories.");
        println!("Uncommitted changes are kept on a {BOLD}saved-{}{RESET} bookmark.", self.doomed);
        if self.current_session.as_deref() == Some(self.doomed.as_str()) {
            println!("{ORANGE}This is the current session: you will be switched away and it will be killed.{RESET}");
        } else if self.is_live(&self.doomed) {
            println!("{ORANGE}Its running session will be killed.{RESET}");
        }
        println!();
        if self.busy() {
            println!("{DIM}deleting…{RESET}");
        } else {
            println!("{BOLD}y{RESET} delete, {BOLD}n{RESET} cancel");
        }
    }

    fn render_name(&self) {
        println!("{BOLD}New workspace{RESET} — ⏎ continue, esc back");
        println!("{BOLD}name>{RESET} {}█", self.name_input);
        println!();
        self.render_status();
    }

    fn render_repos(&self, rows: usize, cols: usize) {
        let verb = if self.is_new { "create" } else { "apply" };
        println!("{BOLD}{}{RESET} — tab toggle, ⏎ {verb}, esc back", self.target);
        println!("{BOLD}>{RESET} {}█", self.query);
        println!();
        self.render_status();
        for failure in &self.failures {
            println!("{RED}{failure}{RESET}");
        }
        if self.busy() {
            println!("{DIM}working… ({} left){RESET}", self.pending);
            return;
        }
        if self.repos.is_empty() {
            println!("{DIM}no jujutsu repositories found under the configured roots{RESET}");
            return;
        }

        let reserved = 4 + self.failures.len() + usize::from(self.error.is_some());
        let window = scroll_window(self.cursor, self.matches.len(), rows.saturating_sub(reserved));
        for (row, m) in self.matches.iter().enumerate().skip(window.start).take(window.len()) {
            let (glyph, color) = match (self.initial.contains(&m.index), self.selected.contains(&m.index)) {
                (false, true) => ("[+] ", GREEN),
                (true, false) => ("[-] ", RED),
                (true, true) => ("[x] ", ORANGE),
                (false, false) => ("[ ] ", DIM),
            };
            let text = self.repos[m.index].label();
            println!("{}", render_row(glyph, color, &text, &m.char_indices, row == self.cursor, cols));
        }
    }
}

