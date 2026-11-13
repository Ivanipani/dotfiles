mod config;
mod scan;
mod ui;

use std::path::PathBuf;

use zellij_tile::prelude::*;

use config::Config;
use scan::{scan, scan_layouts, Candidate, LayoutOption};
use ui::RowStatus;
use zellij_common::list::{move_cursor, scroll_window};
use zellij_common::{fuzzy, ui::{body_height, draw, header, input, help_key_bar as key_bar, draw_help, DIM, RED, RESET}};

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
    help_scroll: Option<usize>,

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
        if self.help_scroll.is_some() {
            self.render_help(rows, cols);
            return;
        }
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
    fn render_help(&mut self, rows: usize, cols: usize) {
        let mut entries = vec![
            ("", "Navigation"),
            ("↑/↓ or Ctrl-k/j", "Move through results"),
            ("F1", "Open or close this help"),
            ("Ctrl-c", "Close the plugin (outside help)"),
            ("", "Search"),
            ("Type", "Fuzzy-filter the list"),
            ("Backspace", "Remove the last search character"),
        ];
        let title = match self.mode {
            Mode::Directories => {
                entries.extend([
                    ("Ctrl-u", "Clear the search"),
                    ("Ctrl-r", "Rescan projects, layouts and sessions"),
                    ("", "Open a project"),
                    ("Enter", "Switch to a running session, otherwise choose a layout"),
                    ("Esc", "Close the plugin"),
                ]);
                "Help · Projects"
            }
            Mode::Layout { .. } => {
                entries.extend([
                    ("", "Create a session"),
                    ("Enter", "Create the session with the selected layout"),
                    ("Esc", "Return to projects"),
                ]);
                "Help · Layouts"
            }
        };
        draw_help(title, &entries, self.help_scroll.as_mut().unwrap(), rows, cols);
    }

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
                self.refilter();
            }
            Err(err) => self.error = Some(err),
        }
    }

    fn refilter(&mut self) {
        let haystacks: Vec<String> = self.candidates.iter().map(|c| c.display.clone()).collect();
        self.matches = fuzzy::filter_sort(&haystacks, &self.query);
        if self.query.is_empty() {
            let current = &self.current_session;
            let live = &self.live_sessions;
            self.matches.sort_by_key(|m| {
                let name = &self.candidates[m.index].session_name;
                (current.as_ref() != Some(name), !live.contains(name))
            });
        }
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
        if key.bare_key == BareKey::F(1) {
            self.help_scroll = if self.help_scroll.is_some() { None } else { Some(0) };
            return true;
        }
        if let Some(offset) = &mut self.help_scroll {
            match key.bare_key {
                BareKey::Esc => self.help_scroll = None,
                BareKey::Down => *offset = offset.saturating_add(1),
                BareKey::Up => *offset = offset.saturating_sub(1),
                _ => {},
            }
            return true;
        }
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
        let action = self.matches.get(self.cursor).map(|m| {
            if self.row_status(&self.candidates[m.index].session_name) == RowStatus::Inactive {
                "choose layout"
            } else { "switch" }
        }).unwrap_or("open");
        let footer = key_bar(&[("⏎", action), ("↑↓", "move"), ("esc", "quit"), ("^u", "clear"), ("^r", "rescan")], cols);
        let mut body = vec![input("search", 6, &self.query), format!("{DIM}◉ current · ● running · ○ inactive | active first{RESET}")];
        if let Some(err) = &self.error {
            body.push(format!("{RED}{err}{RESET}"));
        } else if !self.permissions_granted {
            body.push(format!("{DIM}waiting for permissions…{RESET}"));
        } else if !self.host_ready {
            body.push(format!("{DIM}waiting for host filesystem…{RESET}"));
        } else if self.candidates.is_empty() {
            body.push(format!("{DIM}no project directories found under the configured roots{RESET}"));
        } else if self.matches.is_empty() {
            body.push(format!("{DIM}No matching projects. Ctrl-u clears the search.{RESET}"));
        } else {
            let height = body_height(rows).saturating_sub(footer.lines().count().saturating_sub(1) + body.len());
            let window = scroll_window(self.cursor, self.matches.len(), height);
            for (row, m) in self.matches.iter().enumerate().skip(window.start).take(window.len()) {
                let candidate = &self.candidates[m.index];
                body.push(ui::render_row(&candidate.display, &m.char_indices, self.row_status(&candidate.session_name), row == self.cursor, cols));
            }
        }
        let right = format!("{}/{} projects", self.matches.len(), self.candidates.len());
        draw(&header("Sessionizer", &right, cols), &body, &footer, rows, cols);
    }

    fn render_layouts(&self, rows: usize, cols: usize, session_name: &str) {
        let footer = key_bar(&[("⏎", "create session"), ("↑↓", "move"), ("esc", "back")], cols);
        let mut body = vec![input("search", 6, &self.layout_query)];
        if let Some(error) = &self.layout_error {
            body.push(format!("{RED}{error}{RESET}"));
        }
        if self.layout_matches.is_empty() {
            body.push(format!("{DIM}No matching layouts. Backspace to edit the search.{RESET}"));
        }
        let height = body_height(rows).saturating_sub(footer.lines().count().saturating_sub(1) + body.len());
        let window = scroll_window(self.layout_cursor, self.layout_matches.len(), height);
        for (row, m) in self.layout_matches.iter().enumerate().skip(window.start).take(window.len()) {
            body.push(ui::render_row(&self.layouts[m.index].name, &m.char_indices, RowStatus::Inactive, row == self.layout_cursor, cols));
        }
        draw(&header("Choose layout", session_name, cols), &body, &footer, rows, cols);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_orders_browsing_but_search_keeps_best_match_first() {
        let mut state = State {
            candidates: ["alpha", "beta", "gamma"].into_iter().map(|name| Candidate {
                display: name.into(), session_name: name.into(),
            }).collect(),
            current_session: Some("gamma".into()),
            live_sessions: vec!["beta".into(), "gamma".into()],
            ..State::default()
        };
        state.refilter();
        assert_eq!(state.matches.iter().map(|m| m.index).collect::<Vec<_>>(), vec![2, 1, 0]);
        state.query = "alpha".into();
        state.refilter();
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.matches[0].index, 0);
    }
}

#[cfg(test)]
mod help_tests {
    use super::*;

    #[test]
    fn help_preserves_search_and_selection_and_consumes_actions() {
        let mut state = State {
            mode: Mode::Directories,
            query: "project".into(),
            cursor: 3,
            ..State::default()
        };
        state.handle_key(KeyWithModifier::new(BareKey::F(1)));
        assert_eq!(state.help_scroll, Some(0));
        for key in [BareKey::Char('d'), BareKey::Tab, BareKey::Enter, BareKey::Backspace, BareKey::Down] {
            state.handle_key(KeyWithModifier::new(key));
        }
        assert_eq!(state.help_scroll, Some(1));
        state.handle_key(KeyWithModifier::new(BareKey::Esc));
        assert_eq!(state.help_scroll, None);
        assert_eq!(state.query, "project");
        assert_eq!(state.cursor, 3);
        assert!(matches!(state.mode, Mode::Directories));
        state.handle_key(KeyWithModifier::new(BareKey::F(1)));
        state.handle_key(KeyWithModifier::new(BareKey::F(1)));
        assert_eq!(state.help_scroll, None);
    }
}
