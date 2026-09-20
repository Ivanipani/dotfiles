mod config;
mod scan;
mod ui;

use std::path::PathBuf;

use zellij_tile::prelude::*;

use config::Config;
use scan::{scan, scan_layouts, Candidate, LayoutOption};
use ui::RowStatus;
use zellij_common::list::{move_cursor, scroll_window};
use zellij_common::{fuzzy, ui::{BOLD, DIM, RED, RESET}};

#[derive(Default)]
enum Mode {
    #[default]
    Directories,
    Layout {
        session_name: String,
        cwd: PathBuf,
    },
}

#[derive(Default)]
struct State {
    config: Config,
    mode: Mode,

    candidates: Vec<Candidate>,
    query: String,
    matches: Vec<fuzzy::Match>,
    cursor: usize,

    layouts: Vec<LayoutOption>,
    layout_error: Option<String>,
    layout_query: String,
    layout_matches: Vec<fuzzy::Match>,
    layout_cursor: usize,

    live_sessions: Vec<String>,
    current_session: Option<String>,

    permissions_granted: bool,
    host_ready: bool,
    error: Option<String>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: std::collections::BTreeMap<String, String>) {
        self.config = Config::from_map(&configuration);

        set_selectable(true);
        subscribe(&[EventType::Key, EventType::PermissionRequestResult, EventType::Visible, EventType::HostFolderChanged, EventType::FailedToChangeHostFolder]);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::FullHdAccess,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(status) => {
                self.permissions_granted = matches!(status, PermissionStatus::Granted);
                if self.permissions_granted {
                    change_host_folder(PathBuf::from("/"));
                } else {
                    self.error = Some("Permissions denied — cannot list directories or sessions.".to_string());
                }
                true
            }
            Event::HostFolderChanged(path) if path == PathBuf::from("/") => {
                self.host_ready = true;
                self.rescan();
                self.refresh_sessions();
                true
            }
            Event::FailedToChangeHostFolder(error) => {
                self.error = Some(error.unwrap_or_else(|| "Cannot access host filesystem".into()));
                true
            }
            Event::Visible(true) => {
                if self.host_ready {
                    self.refresh_sessions();
                }
                true
            }
            Event::Key(key) if self.host_ready || matches!(key.bare_key, BareKey::Esc) => self.handle_key(key),
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        match &self.mode {
            Mode::Directories => self.render_directories(rows, cols),
            Mode::Layout { session_name, .. } => {
                let session_name = session_name.clone();
                self.render_layouts(rows, cols, &session_name);
            }
        }
    }
}

impl State {
    fn rescan(&mut self) {
        if !self.host_ready {
            return;
        }
        (self.layouts, self.layout_error) = scan_layouts(&self.config.layout_dir);
        self.candidates = scan(&self.config);
        self.refilter();
    }

    fn refresh_sessions(&mut self) {
        match get_session_list() {
            Ok(snapshot) => {
                self.current_session =
                    snapshot.live_sessions.iter().find(|s| s.is_current_session).map(|s| s.name.clone());
                self.live_sessions = snapshot.live_sessions.into_iter().map(|s| s.name).collect();
                self.error = None;
            }
            Err(err) => self.error = Some(err),
        }
    }

    fn refilter(&mut self) {
        let haystacks: Vec<String> = self.candidates.iter().map(|c| c.display.clone()).collect();
        self.matches = fuzzy::filter_sort(&haystacks, &self.query);
        self.cursor = 0;
    }

    fn refilter_layouts(&mut self) {
        let haystacks: Vec<String> = self.layouts.iter().map(|l| l.name.clone()).collect();
        self.layout_matches = fuzzy::filter_sort(&haystacks, &self.layout_query);
        self.layout_cursor = 0;
    }

    fn row_status(&self, session_name: &str) -> RowStatus {
        if self.current_session.as_deref() == Some(session_name) {
            RowStatus::Current
        } else if self.live_sessions.iter().any(|s| s == session_name) {
            RowStatus::Active
        } else {
            RowStatus::Inactive
        }
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let ctrl = key.key_modifiers.contains(&KeyModifier::Ctrl);
        match &self.mode {
            Mode::Directories => match key.bare_key {
                BareKey::Esc => {
                    close_self();
                    true
                }
                BareKey::Char('c') if ctrl => {
                    close_self();
                    true
                }
                BareKey::Char('r') if ctrl => {
                    self.rescan();
                    self.refresh_sessions();
                    true
                }
                BareKey::Char('u') if ctrl => {
                    self.query.clear();
                    self.refilter();
                    true
                }
                BareKey::Down => self.move_cursor(1),
                BareKey::Char('j') if ctrl => self.move_cursor(1),
                BareKey::Up => self.move_cursor(-1),
                BareKey::Char('k') if ctrl => self.move_cursor(-1),
                BareKey::Backspace => {
                    self.query.pop();
                    self.refilter();
                    true
                }
                BareKey::Enter => self.select_directory(),
                BareKey::Char(c) if !ctrl => {
                    self.query.push(c);
                    self.refilter();
                    true
                }
                _ => false,
            },
            Mode::Layout { .. } => match key.bare_key {
                BareKey::Esc => {
                    self.mode = Mode::Directories;
                    self.refresh_sessions();
                    true
                }
                BareKey::Char('c') if ctrl => {
                    close_self();
                    true
                }
                BareKey::Down => self.move_layout_cursor(1),
                BareKey::Char('j') if ctrl => self.move_layout_cursor(1),
                BareKey::Up => self.move_layout_cursor(-1),
                BareKey::Char('k') if ctrl => self.move_layout_cursor(-1),
                BareKey::Backspace => {
                    self.layout_query.pop();
                    self.refilter_layouts();
                    true
                }
                BareKey::Enter => self.select_layout(),
                BareKey::Char(c) if !ctrl => {
                    self.layout_query.push(c);
                    self.refilter_layouts();
                    true
                }
                _ => false,
            },
        }
    }

    fn move_cursor(&mut self, delta: isize) -> bool {
        move_cursor(&mut self.cursor, self.matches.len(), delta)
    }

    fn move_layout_cursor(&mut self, delta: isize) -> bool {
        move_cursor(&mut self.layout_cursor, self.layout_matches.len(), delta)
    }

    fn select_directory(&mut self) -> bool {
        let Some(m) = self.matches.get(self.cursor) else { return false };
        let candidate = self.candidates[m.index].clone();

        if self.live_sessions.iter().any(|s| s == &candidate.session_name) {
            switch_session(Some(&candidate.session_name));
            close_self();
            return true;
        }

        self.layout_query.clear();
        self.refilter_layouts();
        self.mode = Mode::Layout {
            session_name: candidate.session_name,
            cwd: PathBuf::from(candidate.display),
        };
        true
    }

    fn select_layout(&mut self) -> bool {
        let Mode::Layout { session_name, cwd } = &self.mode else { return false };
        let Some(m) = self.layout_matches.get(self.layout_cursor) else { return false };
        let layout = &self.layouts[m.index];

        let layout_info = match &layout.path {
            Some(path) => LayoutInfo::File(path.clone(), LayoutMetadata::default()),
            None => LayoutInfo::BuiltIn("default".to_string()),
        };
        switch_session_with_layout(Some(session_name.as_str()), layout_info, Some(cwd.clone()));
        close_self();
        true
    }

    fn render_directories(&self, rows: usize, cols: usize) {
        println!("{}Sessionizer{} — type to filter, ↑/↓ move, ⏎ select, esc quit", BOLD, RESET);
        println!("{}>{} {}█", BOLD, RESET, self.query);
        println!();

        if let Some(err) = &self.error {
            println!("{}{err}{}", RED, RESET);
            return;
        }
        if !self.permissions_granted {
            println!("{}waiting for permissions…{}", DIM, RESET);
            return;
        }
        if !self.host_ready {
            println!("{}waiting for host filesystem…{}", DIM, RESET);
            return;
        }
        if self.candidates.is_empty() {
            println!("{}no project directories found under the configured roots{}", DIM, RESET);
            return;
        }

        let list_height = rows.saturating_sub(4);
        let window = scroll_window(self.cursor, self.matches.len(), list_height);
        for (row, m) in self.matches.iter().enumerate().skip(window.start).take(window.len()) {
            let candidate = &self.candidates[m.index];
            let status = self.row_status(&candidate.session_name);
            let selected = row == self.cursor;
            println!("{}", ui::render_row(&candidate.display, &m.char_indices, status, selected, cols));
        }
    }

    fn render_layouts(&self, rows: usize, cols: usize, session_name: &str) {
        println!("{}Layout for {session_name}{} — ↑/↓ move, ⏎ create, esc back", BOLD, RESET);
        println!("{}>{} {}█", BOLD, RESET, self.layout_query);
        match &self.layout_error {
            Some(error) => println!("{RED}{error}{RESET}"),
            None => println!(),
        }

        let list_height = rows.saturating_sub(4);
        let window = scroll_window(self.layout_cursor, self.layout_matches.len(), list_height);
        for (row, m) in self.layout_matches.iter().enumerate().skip(window.start).take(window.len()) {
            let layout = &self.layouts[m.index];
            let selected = row == self.layout_cursor;
            println!("{}", ui::render_row(&layout.name, &m.char_indices, RowStatus::Inactive, selected, cols));
        }
    }
}
