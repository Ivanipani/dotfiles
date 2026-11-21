//! TUI state and key handling. No terminal I/O here, so it is unit-testable.
//!
//! Keys avoid what Rex binds globally: ctrl+a (prefix), ctrl+h/j/k/l, and every alt+ chord.

use claw::widgets::{Outcome, PickerState, TextInput};
use claw::{Choice, FuzzyFinder, Match};
use crate::jj;
use crate::manifest::{session_label, validate_name, Env, Project};
use crate::ops;
use crate::repos;
use crate::rex::{Rex, HUB_LABEL, TOGGLE_ACTION};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    New,
    Add,
    Remove,
    Delete,
}

/// What the TUI should do as soon as it has loaded.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    /// Set to focus; defaults to the session's own set, else the first.
    pub select: Option<String>,
    pub action: Option<Action>,
    /// Name for `Action::New`, skipping the name prompt.
    pub new_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub name: String,
    pub source: Option<String>,
    pub status: Option<jj::Status>,
    /// False until the background load answers.
    pub loaded: bool,
}

#[derive(Debug, Clone)]
pub struct SetRow {
    pub name: String,
    pub projects: Vec<ProjectRow>,
    pub session: Option<String>,
    pub tabs: Option<usize>,
    pub updated: Option<SystemTime>,
}

impl SetRow {
    pub fn dirty(&self) -> usize {
        self.projects.iter().filter(|p| p.status.as_ref().is_some_and(|s| s.dirty)).count()
    }

    pub fn loading(&self) -> bool {
        self.projects.iter().any(|p| !p.loaded)
    }

    /// Fuzzy haystack: the set name, then its projects.
    fn haystack(&self) -> String {
        let projects: Vec<_> = self.projects.iter().map(|p| p.name.as_str()).collect();
        format!("{} {}", self.name, projects.join(" "))
    }
}

pub enum Detail {
    Status { set: String, project: String, status: Option<jj::Status> },
    Tabs { set: String, tabs: Option<usize> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sets,
    Projects,
}

pub enum Job {
    Open(String),
    Create { set: String, picks: Vec<Project> },
    Add { set: String, picks: Vec<Project> },
    Remove { set: String, names: Vec<String> },
    Delete(String),
}

impl Job {
    pub fn describe(&self) -> String {
        match self {
            Job::Open(s) => format!("opening {s}…"),
            Job::Create { set, .. } => format!("creating {set}…"),
            Job::Add { picks, .. } => format!("adding {} repo(s)…", picks.len()),
            Job::Remove { names, .. } => format!("removing {} project(s)…", names.len()),
            Job::Delete(s) => format!("deleting {s}…"),
        }
    }
}

/// A titled picker modal.
pub struct Picker {
    pub title: String,
    pub state: PickerState,
}

impl Picker {
    pub fn new(title: impl Into<String>, choices: Vec<Choice<()>>, multi: bool) -> Self {
        Self { title: title.into(), state: PickerState::new(&choices, multi) }
    }
}

fn choice(label: &str, detail: &str) -> Choice<()> {
    Choice::new((), label).description(detail)
}

pub enum Input {
    NewName,
    /// Naming an extra version of `repos[base]`; the picker it came from is restored afterwards.
    Version { picker: Picker, purpose: Pick, repos: Vec<Project>, base: usize },
}

pub enum Confirm {
    DeleteSet(String),
    RemoveProject { set: String, name: String },
}

pub enum Pick {
    NewRepos(String),
    AddRepos(String),
    RemoveProjects(String),
}

pub enum Modal {
    Help,
    Input { title: String, query: TextInput, purpose: Input },
    Confirm { message: String, purpose: Confirm },
    Pick { picker: Picker, purpose: Pick, repos: Vec<Project> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Filter,
}

pub struct Status {
    pub text: String,
    pub error: bool,
}

pub struct App {
    pub env: Env,
    pub rex: Rex,
    pub sets: Vec<SetRow>,
    pub query: TextInput,
    pub matches: Vec<Match>,
    pub set_table: TableState,
    pub project_table: TableState,
    pub focus: Focus,
    pub mode: Mode,
    pub modal: Option<Modal>,
    pub status: Option<Status>,
    pub pending: Option<Job>,
    pub quit: bool,
    /// Lines to print once the terminal is restored.
    pub notes: Vec<String>,
    pub page: usize,
    /// Running in the shared `workspaces` session: quitting goes back instead of exiting.
    pub hub: bool,
    details: Option<Receiver<Detail>>,
}

impl App {
    pub fn new(env: Env, rex: Rex) -> Self {
        let mut app = Self {
            env,
            rex,
            sets: vec![],
            query: TextInput::default(),
            matches: vec![],
            set_table: TableState::default(),
            project_table: TableState::default(),
            focus: Focus::Sets,
            mode: Mode::Browse,
            modal: None,
            status: None,
            pending: None,
            quit: false,
            notes: vec![],
            page: 10,
            hub: false,
            details: None,
        };
        app.hub = app.rex.current_label().as_deref() == Some(HUB_LABEL);
        app.reload(None);
        app
    }

    // --- data ---------------------------------------------------------------

    /// Re-read sets and sessions, keep the selection by name, and refetch details in the background.
    pub fn reload(&mut self, select: Option<String>) {
        let keep = select.or_else(|| self.current().map(|s| s.name.clone()));
        let sessions = self.rex.sessions();
        let previous = std::mem::take(&mut self.sets);
        self.sets = self
            .env
            .sets()
            .into_iter()
            .map(|m| SetRow {
                session: sessions.iter().find(|s| s.label == session_label(&m.name)).map(|s| s.id.clone()),
                tabs: None,
                updated: std::fs::metadata(self.env.set_dir(&m.name).join("workspace.yaml")).and_then(|m| m.modified()).ok(),
                projects: m
                    .projects
                    .into_iter()
                    .map(|p| {
                        // Keep what is already known so a reload does not blank the table.
                        let old = previous.iter().find(|s| s.name == m.name).and_then(|s| s.projects.iter().find(|o| o.name == p.name));
                        ProjectRow {
                            status: old.and_then(|o| o.status.clone()),
                            loaded: old.is_some_and(|o| o.loaded),
                            name: p.name,
                            source: p.source,
                        }
                    })
                    .collect(),
                name: m.name,
            })
            .collect();
        for set in &mut self.sets {
            set.tabs = previous.iter().find(|p| p.name == set.name).and_then(|p| p.tabs);
        }
        let (focus, project) = (self.focus, self.project_table.selected());
        self.refilter(keep.as_deref());
        // A periodic reload must not yank the cursor out of the projects table.
        let same_set = self.current().map(|s| &s.name) == keep.as_ref();
        if same_set && project.is_some_and(|i| self.current().is_some_and(|s| i < s.projects.len())) {
            self.project_table.select(project);
            self.focus = focus;
        }
        self.spawn_details();
    }

    fn spawn_details(&mut self) {
        let (tx, rx) = mpsc::channel();
        for set in &self.sets {
            let (env, rex, set) = (self.env.clone(), self.rex.clone(), set.clone());
            let tx = tx.clone();
            std::thread::spawn(move || {
                if let Some(id) = &set.session {
                    let _ = tx.send(Detail::Tabs { set: set.name.clone(), tabs: rex.tab_count(id) });
                }
                for p in &set.projects {
                    let status = jj::status(&env.set_dir(&set.name).join(&p.name));
                    let _ = tx.send(Detail::Status { set: set.name.clone(), project: p.name.clone(), status });
                }
            });
        }
        self.details = Some(rx);
    }

    pub fn drain_details(&mut self) {
        let Some(rx) = &self.details else { return };
        let received: Vec<_> = rx.try_iter().collect();
        for detail in received {
            match detail {
                Detail::Tabs { set, tabs } => {
                    if let Some(s) = self.sets.iter_mut().find(|s| s.name == set) {
                        s.tabs = tabs;
                    }
                }
                Detail::Status { set, project, status } => {
                    let row = self.sets.iter_mut().find(|s| s.name == set).and_then(|s| s.projects.iter_mut().find(|p| p.name == project));
                    if let Some(row) = row {
                        row.status = status;
                        row.loaded = true;
                    }
                }
            }
        }
    }

    // --- selection ----------------------------------------------------------

    pub fn refilter(&mut self, keep: Option<&str>) {
        let mut finder = FuzzyFinder::new(self.sets.iter().map(SetRow::haystack));
        self.matches = finder.search(self.query.text());
        let at = keep.and_then(|k| self.matches.iter().position(|m| self.sets[m.index].name == k)).unwrap_or(0);
        self.set_table.select((!self.matches.is_empty()).then_some(at));
        self.reset_projects();
    }

    fn reset_projects(&mut self) {
        let has = self.current().is_some_and(|s| !s.projects.is_empty());
        self.project_table.select(has.then_some(0));
        if !has {
            self.focus = Focus::Sets;
        }
    }

    pub fn current(&self) -> Option<&SetRow> {
        let m = self.matches.get(self.set_table.selected()?)?;
        self.sets.get(m.index)
    }

    pub fn current_project(&self) -> Option<&ProjectRow> {
        self.current()?.projects.get(self.project_table.selected()?)
    }

    fn move_by(&mut self, amount: isize) {
        let (len, table) = match self.focus {
            Focus::Sets => (self.matches.len(), &mut self.set_table),
            Focus::Projects => (self.current().map_or(0, |s| s.projects.len()), &mut self.project_table),
        };
        if len == 0 {
            return;
        }
        let at = table.selected().unwrap_or(0).saturating_add_signed(amount).min(len - 1);
        table.select(Some(at));
        if self.focus == Focus::Sets {
            self.reset_projects();
        }
    }

    fn jump(&mut self, end: bool) {
        self.move_by(if end { isize::MAX / 2 } else { isize::MIN / 2 });
    }

    // --- launch -------------------------------------------------------------

    pub fn launch(&mut self, launch: Launch) {
        let target = launch.select.or_else(|| ops::current_set(&self.rex)).filter(|n| self.sets.iter().any(|s| &s.name == n));
        if let Some(name) = &target {
            self.refilter(Some(name));
        }
        let needs_set = matches!(launch.action, Some(Action::Add | Action::Remove | Action::Delete));
        if needs_set && target.is_none() {
            return self.fail("not inside a ws- session: select a set, then press the key again");
        }
        match launch.action {
            Some(Action::Open) => self.open_selected(),
            Some(Action::New) => match launch.new_name {
                Some(name) => self.begin_create(name),
                None => self.prompt_new(),
            },
            Some(Action::Add) => self.begin_add(),
            Some(Action::Remove) => self.begin_remove(),
            Some(Action::Delete) => self.begin_delete(),
            None => {}
        }
    }

    // --- actions ------------------------------------------------------------

    /// Quit, or from the shared hub session go back to where the user came from.
    fn leave(&mut self) {
        if !self.hub {
            self.quit = true;
        } else if let Err(e) = self.rex.perform(&[TOGGLE_ACTION]) {
            self.fail(format!("could not leave the workspaces session: {e:#}"));
        }
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.status = Some(Status { text: text.into(), error: true });
    }

    fn selected_name(&mut self) -> Option<String> {
        let name = self.current().map(|s| s.name.clone());
        if name.is_none() {
            self.fail("no set selected: press n to create one");
        }
        name
    }

    fn open_selected(&mut self) {
        if let Some(name) = self.selected_name() {
            self.pending = Some(Job::Open(name));
        }
    }

    fn prompt_new(&mut self) {
        self.prompt_name("");
    }

    fn prompt_name(&mut self, name: &str) {
        self.modal = Some(Modal::Input { title: " New set — name ".into(), query: TextInput::new(name), purpose: Input::NewName });
    }

    fn begin_create(&mut self, name: String) {
        if let Err(e) = validate_name(&name) {
            return self.fail(e.to_string());
        }
        if self.env.exists(&name) {
            return self.fail(format!("{name} already exists"));
        }
        self.pick_repos(Pick::NewRepos(name));
    }

    fn members(&self, set: &str) -> Vec<String> {
        self.env.load(set).map(|m| m.projects).unwrap_or_default().into_iter().map(|p| p.name).collect()
    }

    fn begin_add(&mut self) {
        if let Some(name) = self.selected_name() {
            self.pick_repos(Pick::AddRepos(name));
        }
    }

    fn pick_repos(&mut self, purpose: Pick) {
        let set = match &purpose {
            Pick::NewRepos(s) | Pick::AddRepos(s) => s.clone(),
            Pick::RemoveProjects(_) => return,
        };
        let have = self.members(&set);
        let repos = repos::scan(&self.env.roots);
        if repos.iter().all(|p| have.contains(&p.name)) {
            return self.fail("no repositories left to add");
        }
        // Repositories already in the set stay listed so an extra version (Ctrl-F) can be made of them.
        let choices = repos
            .iter()
            .map(|p| {
                let source = tilde(p.source.as_deref().unwrap_or(""));
                let detail = if have.contains(&p.name) { format!("in set · {source}") } else { source };
                choice(&p.name, &detail)
            })
            .collect();
        let title = match purpose {
            Pick::NewRepos(_) => format!(" New set {set} — choose repositories "),
            _ => format!(" Add to {set} "),
        };
        let mut picker = Picker::new(title, choices, true);
        // Creating a set is a two-step flow: Esc returns to naming it.
        picker.state = picker.state.action('f', "version").back(matches!(purpose, Pick::NewRepos(_)));
        self.modal = Some(Modal::Pick { picker, purpose, repos });
    }

    fn begin_remove(&mut self) {
        let Some(set) = self.current() else { return self.fail("no set selected") };
        let set_name = set.name.clone();
        if set.projects.is_empty() {
            return self.fail(format!("{set_name} has no projects"));
        }
        if self.focus == Focus::Projects {
            if let Some(p) = self.current_project() {
                let name = p.name.clone();
                let message = format!("Remove {name} from {set_name}? Uncommitted work is saved under a bookmark.");
                self.modal = Some(Modal::Confirm { message, purpose: Confirm::RemoveProject { set: set_name, name } });
                return;
            }
        }
        let choices = set.projects.iter().map(|p| choice(&p.name, &tilde(p.source.as_deref().unwrap_or("")))).collect();
        let picker = Picker::new(format!(" Remove from {set_name} "), choices, true);
        self.modal = Some(Modal::Pick { picker, purpose: Pick::RemoveProjects(set_name), repos: vec![] });
    }

    fn begin_delete(&mut self) {
        if let Some(set) = self.current() {
            let message = format!(
                "Delete {}? {} project(s) are forgotten; uncommitted work is saved under bookmarks.",
                set.name,
                set.projects.len()
            );
            let name = set.name.clone();
            self.modal = Some(Modal::Confirm { message, purpose: Confirm::DeleteSet(name) });
        } else {
            self.fail("no set selected");
        }
    }

    // --- keys ---------------------------------------------------------------

    pub fn key(&mut self, key: KeyEvent) {
        if let Some(modal) = self.modal.take() {
            self.modal_key(modal, key);
            return;
        }
        match self.mode {
            Mode::Filter => self.filter_key(key),
            Mode::Browse => self.browse_key(key),
        }
    }

    fn filter_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.query.clear();
                self.mode = Mode::Browse;
                self.refilter(None);
            }
            KeyCode::Enter => self.mode = Mode::Browse,
            KeyCode::Down => self.move_by(1),
            KeyCode::Up => self.move_by(-1),
            KeyCode::Char('n') if ctrl => self.move_by(1),
            KeyCode::Char('p') if ctrl => self.move_by(-1),
            KeyCode::Char('c') if ctrl => self.leave(),
            _ => {
                if self.query.edit(key) {
                    self.refilter(None);
                }
            }
        }
    }

    fn browse_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let half = (self.page / 2).max(1) as isize;
        if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SUPER) {
            return;
        }
        self.status = None;
        match key.code {
            KeyCode::Char('c') if ctrl => self.leave(),
            KeyCode::Char('q') => self.leave(),
            KeyCode::Esc if self.query.text().is_empty() => self.leave(),
            KeyCode::Esc => {
                self.query.clear();
                self.refilter(None);
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_by(-1),
            KeyCode::Char('n') if ctrl => self.move_by(1),
            KeyCode::Char('p') if ctrl => self.move_by(-1),
            KeyCode::Char('d') if ctrl => self.move_by(half),
            KeyCode::Char('u') if ctrl => self.move_by(-half),
            KeyCode::PageDown => self.move_by(self.page as isize),
            KeyCode::PageUp => self.move_by(-(self.page as isize)),
            // Any other ctrl chord is not ours (ctrl+a, ctrl+h/j/k/l belong to Rex).
            KeyCode::Char(_) if ctrl => {}
            KeyCode::Char('g') | KeyCode::Home => self.jump(false),
            KeyCode::Char('G') | KeyCode::End => self.jump(true),
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Sets if self.current().is_some_and(|s| !s.projects.is_empty()) => Focus::Projects,
                    _ => Focus::Sets,
                }
            }
            KeyCode::Char('/') => self.mode = Mode::Filter,
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char('r') => {
                self.reload(None);
                self.status = Some(Status { text: "reloaded".into(), error: false });
            }
            KeyCode::Enter | KeyCode::Char('o') => self.open_selected(),
            KeyCode::Char('n') => self.prompt_new(),
            KeyCode::Char('a') => self.begin_add(),
            KeyCode::Char('x') => self.begin_remove(),
            KeyCode::Char('D') => self.begin_delete(),
            _ => {}
        }
    }

    fn modal_key(&mut self, modal: Modal, key: KeyEvent) {
        match modal {
            Modal::Help => {}
            Modal::Input { purpose: Input::Version { picker, purpose, repos, .. }, query, title } if key.code == KeyCode::Esc => {
                drop((query, title));
                self.modal = Some(Modal::Pick { picker, purpose, repos });
            }
            Modal::Input { title, mut query, purpose } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => match purpose {
                    Input::NewName => self.begin_create(query.text().trim().to_string()),
                    Input::Version { picker, purpose, repos, base } => {
                        self.add_version(picker, purpose, repos, base, query.text().trim())
                    }
                },
                _ => {
                    query.edit(key);
                    self.modal = Some(Modal::Input { title, query, purpose });
                }
            },
            Modal::Confirm { message, purpose } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.pending = Some(match purpose {
                        Confirm::DeleteSet(set) => Job::Delete(set),
                        Confirm::RemoveProject { set, name } => Job::Remove { set, names: vec![name] },
                    })
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => {}
                _ => self.modal = Some(Modal::Confirm { message, purpose }),
            },
            Modal::Pick { mut picker, purpose, repos } => match picker.state.key(key) {
                Outcome::Continue => self.modal = Some(Modal::Pick { picker, purpose, repos }),
                Outcome::Back => {
                    if let Pick::NewRepos(name) = purpose {
                        self.prompt_name(&name);
                    }
                }
                Outcome::Cancel => {}
                Outcome::Action(_, base) => {
                    let title = format!(" Version of {} — name ", repos[base].name);
                    let purpose = Input::Version { picker, purpose, repos, base };
                    self.modal = Some(Modal::Input { title, query: TextInput::default(), purpose });
                }
                Outcome::Accept(indices) => self.accept_pick(purpose, repos, indices),
            },
        }
    }

    /// Adds `<repo>-<feature>` to the picker, marked; back to the picker on a bad or duplicate name.
    fn add_version(&mut self, mut picker: Picker, purpose: Pick, mut repos: Vec<Project>, base: usize, feature: &str) {
        let version = repos[base].with_feature(feature);
        match validate_name(feature) {
            Err(e) => self.fail(e.to_string()),
            Ok(()) if repos.iter().any(|r| r.name == version.name) => self.fail(format!("{} already exists", version.name)),
            Ok(()) => {
                picker.state.push_marked(base, &choice(&version.name, &tilde(version.source.as_deref().unwrap_or(""))));
                repos.push(version);
            }
        }
        self.modal = Some(Modal::Pick { picker, purpose, repos });
    }

    fn accept_pick(&mut self, purpose: Pick, repos: Vec<Project>, indices: Vec<usize>) {
        let have = match &purpose {
            Pick::NewRepos(set) | Pick::AddRepos(set) => self.members(set),
            Pick::RemoveProjects(_) => vec![],
        };
        let picks: Vec<_> = indices.iter().filter_map(|&i| repos.get(i)).filter(|p| !have.contains(&p.name)).cloned().collect();
        if picks.is_empty() && !matches!(purpose, Pick::RemoveProjects(_)) {
            return self.fail("already in the set; Ctrl-F makes another version");
        }
        self.pending = Some(match purpose {
            Pick::NewRepos(set) => Job::Create { set, picks },
            Pick::AddRepos(set) => Job::Add { set, picks },
            Pick::RemoveProjects(set) => {
                let names = self.current().map(|s| indices.iter().filter_map(|&i| s.projects.get(i)).map(|p| p.name.clone()).collect());
                Job::Remove { set, names: names.unwrap_or_default() }
            }
        });
    }

    // --- jobs ---------------------------------------------------------------

    pub fn execute(&mut self, job: Job) {
        let result = match &job {
            Job::Open(set) => ops::open(&self.env, &self.rex, set),
            Job::Create { set, picks } => ops::add(&self.env, &self.rex, set, picks).and_then(|mut notes| {
                notes.extend(ops::open(&self.env, &self.rex, set)?);
                Ok(notes)
            }),
            Job::Add { set, picks } => ops::add(&self.env, &self.rex, set, picks),
            Job::Remove { set, names } => ops::remove(&self.env, &self.rex, set, names),
            Job::Delete(set) => ops::delete(&self.env, &self.rex, set),
        };
        // The hub is shared and long-lived: it stays up after switching away.
        let leaves = !self.hub && matches!(job, Job::Open(_) | Job::Create { .. });
        let select = match &job {
            Job::Create { set, .. } | Job::Add { set, .. } | Job::Remove { set, .. } => Some(set.clone()),
            _ => None,
        };
        match result {
            Ok(notes) if leaves => {
                self.notes.extend(notes);
                self.quit = true;
            }
            Ok(notes) => {
                self.status = Some(Status { text: notes.join(" · "), error: false });
                self.reload(select);
            }
            Err(e) => {
                self.fail(format!("{e:#}"));
                self.reload(select);
            }
        }
    }
}

pub fn tilde(path: &str) -> String {
    let home = crate::manifest::home();
    match Path::new(path).strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn press(app: &mut App, c: char) {
        app.key(key(KeyCode::Char(c)));
    }

    fn app(dir: &Path, sets: &[(&str, &[&str])]) -> App {
        let env = Env { workspaces_dir: dir.to_path_buf(), roots: vec![] };
        for (name, projects) in sets {
            let projects = projects.iter().map(|p| Project { name: (*p).into(), source: None, feature: None }).collect();
            env.save(&Manifest { name: (*name).into(), projects }).unwrap();
        }
        // No Rex server: sessions come back empty.
        std::env::set_var("REX_BIN", "/nonexistent");
        App::new(env, Rex::locate())
    }

    fn selected(app: &App) -> String {
        app.current().unwrap().name.clone()
    }

    #[test]
    fn new_lists_sets_sorted_with_the_first_selected() {
        let tmp = tempfile::tempdir().unwrap();
        let app = app(tmp.path(), &[("zed", &[]), ("abe", &[])]);
        assert_eq!(selected(&app), "abe");
    }

    #[test]
    fn j_moves_the_selection_down() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[]), ("zed", &[])]);
        press(&mut app, 'j');
        assert_eq!(selected(&app), "zed");
    }

    #[test]
    fn k_at_the_top_stays_on_the_first_set() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[]), ("zed", &[])]);
        press(&mut app, 'k');
        assert_eq!(selected(&app), "abe");
    }

    #[test]
    fn capital_g_jumps_to_the_last_set() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[]), ("mid", &[]), ("zed", &[])]);
        press(&mut app, 'G');
        assert_eq!(selected(&app), "zed");
    }

    #[test]
    fn filter_narrows_the_list_to_fuzzy_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[]), ("zed", &[])]);
        press(&mut app, '/');
        press(&mut app, 'z');
        assert_eq!(app.matches.len(), 1);
    }

    #[test]
    fn filter_matches_on_project_names_too() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &["console"]), ("zed", &["reek"])]);
        press(&mut app, '/');
        for c in "reek".chars() {
            press(&mut app, c);
        }
        assert_eq!(selected(&app), "zed");
    }

    #[test]
    fn typing_in_filter_mode_does_not_trigger_commands() {
        // GIVEN filter mode, WHEN 'q' is typed
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        press(&mut app, '/');
        press(&mut app, 'q');
        // THEN it edits the filter instead of quitting
        assert!(!app.quit && app.query.text() == "q");
    }

    #[test]
    fn esc_in_filter_mode_clears_the_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[]), ("zed", &[])]);
        press(&mut app, '/');
        press(&mut app, 'z');
        app.key(key(KeyCode::Esc));
        assert_eq!(app.matches.len(), 2);
    }

    #[test]
    fn q_in_browse_mode_quits() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        press(&mut app, 'q');
        assert!(app.quit);
    }

    #[test]
    fn q_in_the_hub_goes_back_instead_of_quitting() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        app.hub = true;
        press(&mut app, 'q');
        // No Rex server here, so going back fails visibly, but the TUI stays up.
        assert!(!app.quit && app.status.as_ref().is_some_and(|s| s.error));
    }

    #[test]
    fn reload_keeps_the_projects_cursor_where_it_was() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &["one", "two"])]);
        app.key(key(KeyCode::Tab));
        press(&mut app, 'j');
        app.reload(None);
        assert_eq!((app.focus, app.project_table.selected()), (Focus::Projects, Some(1)));
    }

    #[test]
    fn reload_keeps_already_loaded_project_status() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &["one"])]);
        app.sets[0].projects[0].loaded = true;
        app.reload(None);
        assert!(app.sets[0].projects[0].loaded);
    }

    #[test]
    fn alt_chords_are_ignored_so_rex_bindings_keep_working() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[]), ("zed", &[])]);
        app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT));
        assert_eq!(selected(&app), "abe");
    }

    #[test]
    fn tab_focuses_projects_only_when_the_set_has_some() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        app.key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Sets);
    }

    #[test]
    fn tab_moves_focus_to_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &["one"])]);
        app.key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Projects);
    }

    #[test]
    fn enter_queues_opening_the_selected_set() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &["one"])]);
        app.key(key(KeyCode::Enter));
        assert!(matches!(&app.pending, Some(Job::Open(s)) if s == "abe"));
    }

    #[test]
    fn enter_with_no_sets_reports_an_error_instead_of_crashing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[]);
        app.key(key(KeyCode::Enter));
        assert!(app.status.as_ref().is_some_and(|s| s.error) && app.pending.is_none());
    }

    #[test]
    fn capital_d_asks_for_confirmation_before_deleting() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        press(&mut app, 'D');
        assert!(matches!(app.modal, Some(Modal::Confirm { .. })) && app.pending.is_none());
    }

    #[test]
    fn confirming_a_delete_queues_the_job() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        press(&mut app, 'D');
        press(&mut app, 'y');
        assert!(matches!(&app.pending, Some(Job::Delete(s)) if s == "abe"));
    }

    #[test]
    fn declining_a_delete_queues_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        press(&mut app, 'D');
        press(&mut app, 'n');
        assert!(app.pending.is_none() && app.modal.is_none());
    }

    #[test]
    fn new_set_rejects_an_invalid_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[]);
        press(&mut app, 'n');
        for c in "a/b".chars() {
            press(&mut app, c);
        }
        app.key(key(KeyCode::Enter));
        assert!(app.status.as_ref().is_some_and(|s| s.error));
    }

    #[test]
    fn new_set_rejects_a_name_that_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        press(&mut app, 'n');
        for c in "abe".chars() {
            press(&mut app, c);
        }
        app.key(key(KeyCode::Enter));
        assert!(app.status.as_ref().is_some_and(|s| s.text.contains("already exists")));
    }

    #[test]
    fn new_set_ignores_a_bare_directory_without_a_manifest() {
        // GIVEN debris from a failed create
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[]);
        std::fs::create_dir(tmp.path().join("vms")).unwrap();
        press(&mut app, 'n');
        for c in "vms".chars() {
            press(&mut app, c);
        }
        // WHEN
        app.key(key(KeyCode::Enter));
        // THEN it is not rejected as existing
        assert!(!app.status.as_ref().is_some_and(|s| s.text.contains("already exists")));
    }

    #[test]
    fn add_version_marks_a_new_project_named_after_repo_and_feature() {
        // GIVEN a picker over one repo
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[]);
        let repos = vec![Project { name: "api".into(), source: Some("/src/api".into()), feature: None }];
        let picker = Picker::new("t", vec![choice("api", "")], true);
        // WHEN
        app.add_version(picker, Pick::NewRepos("demo".into()), repos, 0, "v2");
        app.key(key(KeyCode::Enter));
        // THEN accepting queues only the version
        assert!(matches!(&app.pending, Some(Job::Create { picks, .. }) if picks.len() == 1 && picks[0].name == "api-v2"));
    }

    fn new_set_picker(app: &mut App) {
        let repos = vec![Project { name: "api".into(), source: None, feature: None }];
        let picker = Picker::new("t", vec![choice("api", "")], true);
        app.modal = Some(Modal::Pick { picker, purpose: Pick::NewRepos("demo".into()), repos });
    }

    #[test]
    fn esc_in_the_new_set_repo_picker_goes_back_to_naming_it() {
        // GIVEN the repository page of creating "demo"
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[]);
        new_set_picker(&mut app);
        // WHEN
        app.key(key(KeyCode::Esc));
        // THEN the name page is back, still holding the name
        assert!(matches!(&app.modal, Some(Modal::Input { purpose: Input::NewName, query, .. }) if query.text() == "demo"));
    }

    #[test]
    fn ctrl_c_in_the_new_set_repo_picker_abandons_the_flow() {
        // GIVEN the repository page of creating "demo"
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[]);
        new_set_picker(&mut app);
        // WHEN
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        // THEN
        assert!(app.modal.is_none());
    }

    #[test]
    fn question_mark_opens_help() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[]);
        press(&mut app, '?');
        assert!(matches!(app.modal, Some(Modal::Help)));
    }

    #[test]
    fn launch_selects_the_named_set() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[]), ("zed", &[])]);
        app.launch(Launch { select: Some("zed".into()), ..Launch::default() });
        assert_eq!(selected(&app), "zed");
    }

    #[test]
    fn launch_add_outside_a_ws_session_reports_an_error_instead_of_crashing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        app.launch(Launch { action: Some(Action::Add), ..Launch::default() });
        assert!(app.status.as_ref().is_some_and(|s| s.error) && app.modal.is_none());
    }

    #[test]
    fn launch_add_for_a_named_set_opens_the_repo_picker_or_reports_none() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path(), &[("abe", &[])]);
        app.launch(Launch { select: Some("abe".into()), action: Some(Action::Add), ..Launch::default() });
        // No roots are configured, so there is nothing to pick: a message, not a modal.
        assert!(app.status.as_ref().is_some_and(|s| s.text.contains("no repositories")));
    }

    #[test]
    fn dirty_counts_only_dirty_projects() {
        let row = |dirty| ProjectRow {
            name: "p".into(),
            source: None,
            status: Some(jj::Status { dirty, ..jj::Status::default() }),
            loaded: true,
        };
        let set = SetRow { name: "s".into(), projects: vec![row(true), row(false)], session: None, tabs: None, updated: None };
        assert_eq!(set.dirty(), 1);
    }
}
