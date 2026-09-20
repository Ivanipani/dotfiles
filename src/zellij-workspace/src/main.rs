mod config;
mod layout;
mod manifest;
mod repos;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use zellij_tile::prelude::*;

use config::Config;
use layout::{compose, tab_names, Project, DEFAULT_TEMPLATE, FILE_NAME as LAYOUT_FILE_NAME};
use manifest::{Component, Manifest};
use repos::{list_workspaces, members, missing_sources, sanitize_name, scan_repos, workspace_path, workspaces_root, Repo};
use zellij_common::host::{mounted_path, HOST_MOUNT};
use zellij_common::list::{move_cursor, scroll_window};
use zellij_common::ui::{body_height, draw, header, input, key_bar, render_row, BOLD, DIM, GREEN, ORANGE, RED, RESET};
use zellij_common::fuzzy;

/// `$1` source repository, `$2` workspace name, `$3` destination.
const ADD_SCRIPT: &str = r#"mkdir -p "$(dirname "$3")" && jj -R "$1" workspace add --name "$2" "$3""#;
/// `$1` is `one` (`$2` a workspace directory) or `set` (`$2…` set directories
/// of them). A workspace whose source repository is gone is just deleted. A workspace with uncommitted changes is first kept alive by a
/// `saved-<set>` bookmark (suffixed `-2`, `-3`, … if taken); if that or the
/// forget fails, nothing is deleted. Prints `saved <bookmark>` for each.
const WORKSPACE_SCRIPT: &str = r#"
forget() {
  d=$1
  jj -R "$d" log -r @ --no-graph -T empty >/dev/null 2>&1 || return 0
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
  set) shift; for s; do for d in "$s"/*/; do [ -d "$d" ] || continue; forget "$d" || exit 1; done; rm -rf "$s"; done ;;
esac
"#;
/// Zellij session name of a workspace set; the prefix keeps it from colliding
/// with unrelated sessions of the same name.
fn session_name(set: &str) -> String {
    format!("ws-{set}")
}

/// `$1` the session name. Detached and delayed so it outlives, and runs after
/// leaving, the session it removes.
const KILL_SESSION_SCRIPT: &str = r#"(sleep 1; zellij delete-session --force "$1") >/dev/null 2>&1 &"#;

#[derive(Default, PartialEq)]
enum Mode {
    #[default]
    Home,
    Name,
    Repos,
    /// Naming an extra workspace of the repository under the cursor.
    Feature,
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
    /// The workspace sets awaiting delete confirmation.
    doomed: Vec<String>,
    /// Why each stale workspace set is considered so.
    stale: BTreeMap<String, String>,
    /// Bookmarks created for unsaved changes by the last delete.
    notes: Vec<String>,

    /// The repo (index) a feature workspace is being named for.
    feature_of: usize,
    /// Names added in the current feature prompt.
    features_added: Vec<String>,
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
    /// Sessions that exited but can still be resurrected.
    resurrectable_sessions: Vec<String>,
    current_session: Option<String>,
    tabs: Vec<TabInfo>,

    template: String,
    /// The set's `workspace.yaml`, kept in step with the jobs of the last apply.
    manifest: Manifest,
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
            Mode::Name => self.render_name(rows, cols),
            Mode::Repos => self.render_repos(rows, cols),
            Mode::Feature => self.render_feature(rows, cols),
            Mode::Confirm => self.render_confirm(rows, cols),
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
        let sessions_known = match get_session_list() {
            Ok(snapshot) => {
                self.current_session =
                    snapshot.live_sessions.iter().find(|s| s.is_current_session).map(|s| s.name.clone());
                self.live_sessions = snapshot.live_sessions.into_iter().map(|s| s.name).collect();
                self.resurrectable_sessions = snapshot.resurrectable_sessions.into_iter().map(|(n, _)| n).collect();
                true
            }
            Err(_) => false,
        };
        self.stale = self
            .workspaces
            .iter()
            .filter_map(|set| {
                let mut reasons = Vec::new();
                if sessions_known && !self.has_session(set) {
                    reasons.push("no session".to_string());
                }
                let missing = missing_sources(&self.config, Self::mount(), set);
                if !missing.is_empty() {
                    reasons.push(format!("source missing: {}", missing.join(", ")));
                }
                (!reasons.is_empty()).then(|| (set.clone(), reasons.join("; ")))
            })
            .collect();
        // Start on the workspace this session belongs to, else on "new".
        let current = self.workspaces.iter().position(|w| self.is_current(w));
        self.home_cursor = current.map_or(0, |i| i + 1);
    }

    fn is_live(&self, set: &str) -> bool {
        let session = session_name(set);
        self.live_sessions.iter().any(|s| *s == session)
    }

    fn is_current(&self, set: &str) -> bool {
        self.current_session.as_deref() == Some(session_name(set).as_str())
    }

    /// Whether `set` has a running or resurrectable session.
    fn has_session(&self, set: &str) -> bool {
        let session = session_name(set);
        self.is_live(set) || self.resurrectable_sessions.iter().any(|s| *s == session)
    }

    /// Whether tab changes for `target` should be made in this very session.
    fn syncs_tabs(&self) -> bool {
        self.is_current(&self.target)
    }

    fn enter_repos(&mut self, target: String, is_new: bool) {
        let mut repos = scan_repos(&self.config, Self::mount());
        let present = members(&self.config, Self::mount(), &target);
        let manifest = Manifest::load(&self.config, Self::mount(), &target);
        for name in &present {
            if !repos.iter().any(|r| &r.name == name) {
                let source = manifest.as_ref().and_then(|m| m.projects.iter().find(|c| &c.name == name)).and_then(|c| c.source.clone());
                repos.push(Repo { name: name.clone(), source, feature: None });
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
            Mode::Feature => self.handle_feature_key(key.bare_key, ctrl),
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
                    self.confirm_delete(vec![self.workspaces[i].clone()]);
                    true
                }
                None => false,
            },
            BareKey::Char('c') => {
                if self.stale.is_empty() {
                    self.error = Some("No stale workspaces.".into());
                } else {
                    self.confirm_delete(self.stale.keys().cloned().collect());
                }
                true
            }
            BareKey::Enter => {
                self.error = None;
                match self.home_cursor.checked_sub(1) {
                    None => {
                        self.name_input.clear();
                        self.mode = Mode::Name;
                    }
                    Some(i) => {
                        let name = self.workspaces[i].clone();
                        if self.is_live(&name) && !self.is_current(&name) {
                            switch_session(Some(&session_name(&name)));
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

    fn confirm_delete(&mut self, sets: Vec<String>) {
        self.notes.clear();
        self.doomed = sets;
        self.error = None;
        self.mode = Mode::Confirm;
    }

    fn handle_confirm_key(&mut self, key: BareKey) -> bool {
        match key {
            BareKey::Char('y') => {
                let dirs: Vec<String> =
                    self.doomed.iter().map(|d| workspace_path(&self.config, d, None).to_string_lossy().into_owned()).collect();
                let mut args = vec!["sh", "-c", WORKSPACE_SCRIPT, "sh", "set"];
                args.extend(dirs.iter().map(String::as_str));
                let context = BTreeMap::from([("op".to_string(), "delete".to_string())]);
                run_command(&args, context);
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

    /// The sets' directories are gone. Leave the current session if it is one
    /// of them, then have their sessions deleted (killed, if running) from outside.
    fn sets_deleted(&mut self) {
        let doomed = self.doomed.clone();
        for set in doomed.iter().filter(|s| self.has_session(s)) {
            run_command(&["sh", "-c", KILL_SESSION_SCRIPT, "sh", &session_name(set)], BTreeMap::new());
        }
        let in_doomed = |s: &String| doomed.iter().any(|d| session_name(d) == *s);
        if self.current_session.as_ref().is_some_and(in_doomed) {
            let other = self.live_sessions.iter().find(|s| !in_doomed(s)).cloned();
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
            BareKey::Char('f') if ctrl => self.start_feature(),
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

    fn start_feature(&mut self) -> bool {
        let Some(m) = self.matches.get(self.cursor) else { return false };
        if self.repos[m.index].source.is_none() {
            self.error = Some("Source repository not found.".into());
            return true;
        }
        self.feature_of = m.index;
        self.features_added.clear();
        self.name_input.clear();
        self.error = None;
        self.mode = Mode::Feature;
        true
    }

    fn end_features(&mut self) -> bool {
        self.mode = Mode::Repos;
        self.error = None;
        true
    }

    /// Adds, selected, workspaces of the same repository named `<repo>-<feature>`,
    /// one per line entered, until an empty line, esc or ctrl-d.
    fn handle_feature_key(&mut self, key: BareKey, ctrl: bool) -> bool {
        match key {
            BareKey::Esc => self.end_features(),
            BareKey::Char('d') if ctrl => self.end_features(),
            BareKey::Backspace => {
                self.name_input.pop();
                true
            }
            BareKey::Enter => {
                let feature = sanitize_name(&self.name_input);
                let base = &self.repos[self.feature_of];
                let name = format!("{}-{feature}", base.name);
                if feature.is_empty() {
                    return self.end_features();
                } else if self.repos.iter().any(|r| r.name == name) {
                    self.error = Some(format!("'{name}' already exists."));
                } else {
                    let repo = Repo { name, source: base.source.clone(), feature: Some(feature) };
                    let index = self.repos.len();
                    self.features_added.push(repo.name.clone());
                    self.repos.push(repo);
                    self.selected.insert(index);
                    self.query.clear();
                    self.error = None;
                    self.refilter();
                    self.cursor = self.matches.iter().position(|m| m.index == index).unwrap_or(0);
                    self.name_input.clear();
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

    fn toggle_current(&mut self) -> bool {
        let Some(m) = self.matches.get(self.cursor) else { return false };
        if !self.selected.remove(&m.index) {
            self.selected.insert(m.index);
        }
        self.error = None;
        true
    }

    fn component(&self, repo: &Repo) -> Component {
        Component { name: repo.name.clone(), source: repo.source.clone() }
    }

    /// Layout with two tabs for every project in the manifest.
    fn layout_kdl(&self) -> Result<String, String> {
        let projects: Vec<Project> = self
            .manifest
            .projects
            .iter()
            .map(|c| Project { name: c.name.clone(), dir: self.project_dir(&c.name) })
            .collect();
        compose(&self.template, &projects)
    }

    /// Saves `workspace.yaml` and the layout beside it, so both follow the members.
    fn save_manifest(&mut self) {
        let layout_file = workspaces_root(&self.config, Self::mount()).map(|root| root.join(&self.target).join(LAYOUT_FILE_NAME));
        let result = self.manifest.save(&self.config, Self::mount()).and_then(|_| {
            let kdl = self.layout_kdl()?;
            let file = layout_file.ok_or("workspaces_dir is not accessible")?;
            std::fs::write(file, kdl).map_err(|e| format!("cannot write {LAYOUT_FILE_NAME}: {e}"))
        });
        if let Err(err) = result {
            self.failures.push(err);
        }
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
    /// adds become `jj workspace add` (plus its tabs), removals close the tab,
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

        let mut manifest = Manifest::load(&self.config, Self::mount(), &self.target).unwrap_or_else(|| Manifest {
            name: self.target.clone(),
            projects: self.initial.iter().map(|&i| self.component(&self.repos[i])).collect(),
        });
        for &i in self.selected.difference(&self.initial) {
            manifest.add(self.component(&self.repos[i]));
        }
        for &i in self.initial.difference(&self.selected) {
            manifest.remove(&self.repos[i].name);
        }
        self.manifest = manifest;
        self.save_manifest();

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
                match &repo.feature {
                    Some(feature) => format!("{}-{feature}", self.target),
                    None => self.target.clone(),
                },
                self.project_dir(&repo.name).to_string_lossy().into_owned(),
            ]);
            jobs.push((args.collect(), job_context("add", &repo.name)));
        }
        for &i in self.initial.difference(&self.selected) {
            let repo = &self.repos[i];
            if sync {
                let names: Vec<String> = tab_names(&repo.name).collect();
                self.tabs.iter().filter(|t| names.contains(&t.name)).for_each(|t| close_tab_with_id(t.tab_id as u64));
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
                self.sets_deleted();
            } else {
                let detail = String::from_utf8_lossy(stderr);
                self.error = Some(format!("delete {}: {}", self.doomed.join(", "), detail.trim()));
                self.reload_home();
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
            // The job did not happen, so the manifest must not claim it did.
            if op == "add" {
                self.manifest.remove(project);
            } else if let Some(repo) = self.repos.iter().find(|r| &r.name == project) {
                self.manifest.add(self.component(repo));
            }
            self.save_manifest();
        }
        self.pending = self.pending.saturating_sub(1);
        if self.pending == 0 {
            self.finish();
        }
        true
    }

    /// All jobs are done: leave, or stay to show what failed. A set that is new
    /// (or not running) also gets its session, with the stored layout.
    fn finish(&mut self) {
        if !self.failures.is_empty() {
            return;
        }
        if !self.syncs_tabs() {
            match self.layout_kdl() {
                Ok(kdl) => switch_session_with_layout(
                    Some(&session_name(&self.target)),
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

    /// Permission / host / error lines; `true` if the plugin isn't ready for its normal body.
    fn status_lines(&self, body: &mut Vec<String>) -> bool {
        if let Some(err) = &self.error {
            body.push(format!("{RED}{err}{RESET}"));
        }
        if !self.permissions_granted {
            body.push(format!("{DIM}waiting for permissions…{RESET}"));
            return true;
        }
        if !self.host_ready {
            body.push(format!("{DIM}waiting for host filesystem…{RESET}"));
            return true;
        }
        false
    }

    fn render_home(&self, rows: usize, cols: usize) {
        let mut body: Vec<String> = self.notes.iter().map(|n| format!("{GREEN}{n}{RESET}")).collect();
        let ready = !self.status_lines(&mut body);
        if ready {
            let width = self.workspaces.iter().map(|w| w.chars().count()).max().unwrap_or(0);
            let mut entries = vec![("  ", DIM, "new workspace".to_string())];
            for name in &self.workspaces {
                let (glyph, color, state) = if self.current_session.as_deref() == Some(name.as_str()) {
                    ("◉ ", GREEN, "current")
                } else if self.is_live(name) {
                    ("● ", ORANGE, "running")
                } else {
                    ("  ", DIM, "")
                };
                let stale = self.stale.get(name).map(|r| format!("  stale ({r})")).unwrap_or_default();
                entries.push((glyph, color, format!("{name:<width$}  {state}{stale}").trim_end().to_string()));
            }
            entries[0].2 = format!("+ {}", entries[0].2);
            let window = scroll_window(self.home_cursor, entries.len(), body_height(rows).saturating_sub(body.len()));
            for i in window {
                let (glyph, color, text) = &entries[i];
                body.push(render_row(glyph, color, text, &[], i == self.home_cursor, cols));
            }
        }
        let hints = [("↑↓", "move"), ("⏎", "open"), ("d", "delete"), ("c", "clean stale"), ("^r", "refresh"), ("esc", "quit")];
        let right = format!("{} sets · {} stale", self.workspaces.len(), self.stale.len());
        draw(&header("Workspaces", &right, cols), &body, &key_bar(&hints, cols), rows, cols);
    }

    fn render_confirm(&self, rows: usize, cols: usize) {
        let count: usize = self.doomed.iter().map(|d| members(&self.config, Self::mount(), d).len()).sum();
        let mut body = vec![
            String::new(),
            format!("Forgets {count} jj workspace(s) and removes their directories."),
            format!("Uncommitted changes are kept on a {BOLD}saved-<set>{RESET} bookmark."),
        ];
        if self.doomed.len() > 1 {
            body.extend(self.doomed.iter().map(|d| format!("  {d}")));
        }
        if self.current_session.as_ref().is_some_and(|c| self.doomed.contains(c)) {
            body.push(format!("{ORANGE}This is the current session: you will be switched away and it will be killed.{RESET}"));
        } else if self.doomed.iter().any(|d| self.is_live(d)) {
            body.push(format!("{ORANGE}Running sessions will be killed.{RESET}"));
        }
        let title = match self.doomed.as_slice() {
            [one] => format!("Delete '{one}'?"),
            many => format!("Delete {} stale sets?", many.len()),
        };
        let footer = if self.busy() {
            format!("{DIM}deleting…{RESET}")
        } else {
            key_bar(&[("y", "delete"), ("n", "cancel"), ("esc", "cancel")], cols)
        };
        draw(&header(&title, "", cols), &body, &footer, rows, cols);
    }

    fn render_name(&self, rows: usize, cols: usize) {
        let mut body = vec![String::new(), input("name", 4, &self.name_input)];
        self.status_lines(&mut body);
        let hints = [("⏎", "continue"), ("esc", "back")];
        draw(&header("New workspace", "", cols), &body, &key_bar(&hints, cols), rows, cols);
    }

    fn render_feature(&self, rows: usize, cols: usize) {
        let repo = &self.repos[self.feature_of];
        let mut body = vec![String::new(), input("feature", 7, &self.name_input)];
        self.status_lines(&mut body);
        body.extend(self.features_added.iter().map(|n| format!("{GREEN}[+] {n}{RESET}")));
        let hints = [("⏎", "add"), ("⏎ empty", "done"), ("^d", "done"), ("esc", "done")];
        draw(&header(&format!("Feature workspace of {}", repo.name), "", cols), &body, &key_bar(&hints, cols), rows, cols);
    }

    fn render_repos(&self, rows: usize, cols: usize) {
        let verb = if self.is_new { "create" } else { "apply" };
        let right = format!("{} selected · {}/{} shown", self.selected.len(), self.matches.len(), self.repos.len());
        let mut body = vec![input("filter", 6, &self.query)];
        body.push(format!("{GREEN}[+] add{RESET}  {RED}[-] remove{RESET}  {ORANGE}[x] kept{RESET}  {DIM}[ ] not in set{RESET}"));
        let ready = !self.status_lines(&mut body);
        body.extend(self.failures.iter().map(|f| format!("{RED}{f}{RESET}")));
        if ready {
            if self.busy() {
                body.push(format!("{DIM}working… ({} left){RESET}", self.pending));
            } else if self.repos.is_empty() {
                body.push(format!("{DIM}no jujutsu repositories found under the configured roots{RESET}"));
            } else {
                self.repo_rows(&mut body, rows, cols);
            }
        }
        let hints = if self.busy() {
            vec![("^c", "close")]
        } else {
            vec![("tab", "toggle"), ("^f", "feature"), ("⏎", verb), ("↑↓", "move"), ("^u", "clear"), ("^r", "rescan"), ("esc", "back")]
        };
        draw(&header(&self.target, &right, cols), &body, &key_bar(&hints, cols), rows, cols);
    }

    /// Appends the matching repos as a name column followed by the source path.
    fn repo_rows(&self, body: &mut Vec<String>, rows: usize, cols: usize) {
        let name_width = self.repos.iter().map(|r| r.name.chars().count()).max().unwrap_or(0);
        let window = scroll_window(self.cursor, self.matches.len(), body_height(rows).saturating_sub(body.len()));
        for (row, m) in self.matches.iter().enumerate().skip(window.start).take(window.len()) {
            let (glyph, color) = match (self.initial.contains(&m.index), self.selected.contains(&m.index)) {
                (false, true) => ("[+] ", GREEN),
                (true, false) => ("[-] ", RED),
                (true, true) => ("[x] ", ORANGE),
                (false, false) => ("[ ] ", DIM),
            };
            let repo = &self.repos[m.index];
            let name_len = repo.name.chars().count();
            let pad = name_width - name_len;
            let detail = match &repo.source {
                Some(path) => path.display().to_string(),
                None => "(source not found)".to_string(),
            };
            let text = format!("{}{}  {detail}", repo.name, " ".repeat(pad));
            // Match indices refer to the unpadded label; shift those past the name.
            let indices: Vec<usize> = m.char_indices.iter().map(|&i| if i < name_len { i } else { i + pad }).collect();
            body.push(render_row(glyph, color, &text, &indices, row == self.cursor, cols));
        }
    }
}
