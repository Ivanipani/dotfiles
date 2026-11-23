//! Rendering: a sets table over a projects table, a fuzzy filter box, and modals, in claw's style.

use super::app::{tilde, App, Focus, Input, Modal, Mode, Pick, ProjectRow, SetRow};
use crate::park::State;
use claw::style::{self, glyph, Theme};
use claw::widgets::{highlight, hints, modal, pane, popup, status, steps};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, Wrap},
    Frame,
};
use std::time::SystemTime;

const MIN_WIDTH: u16 = 50;
const MIN_HEIGHT: u16 = 12;
/// The new-set flow's pages, shown as a breadcrumb in its modals.
const NEW_SET: &[&str] = &["Name", "Repositories"];

fn theme() -> Theme {
    Theme::default()
}

fn header(titles: &[&'static str]) -> Row<'static> {
    Row::new(titles.iter().map(|t| Cell::from(*t))).style(style::heading())
}

fn placeholder() -> Cell<'static> {
    Cell::from(glyph::ABSENT).style(style::muted())
}

fn loading() -> Cell<'static> {
    Cell::from("…").style(style::muted())
}

fn ago(time: Option<SystemTime>) -> String {
    let Some(secs) = time.and_then(|t| SystemTime::now().duration_since(t).ok()).map(|d| d.as_secs()) else {
        return glyph::ABSENT.into();
    };
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(Paragraph::new("Enlarge the pane\nq to quit"), area);
        return;
    }
    // Sets take what they need, up to 40% of the screen; projects get the rest.
    let sets_height = (app.matches.len() as u16 + 3).clamp(5, (area.height * 2 / 5).max(5));
    let [title, search, sets, projects, status_area, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(sets_height),
        Constraint::Min(4),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    app.page = usize::from(sets.height.saturating_sub(4)).max(1);
    render_title(frame, app, title);
    let count = format!(" {}/{} ", app.matches.len(), app.sets.len());
    let filtering = app.mode == Mode::Filter && app.modal.is_none();
    app.query.search(frame, search, &count, filtering, "press / to filter sets and projects", &theme());
    render_sets(frame, app, sets);
    render_projects(frame, app, projects);
    let line = app.status.as_ref().map_or_else(Line::default, |s| status(&s.text, s.error, &theme()));
    frame.render_widget(Paragraph::new(line), status_area);
    frame.render_widget(Paragraph::new(hints(key_hints(app), &theme())), keys);
    if let Some(modal) = &mut app.modal {
        render_modal(frame, modal, area);
    }
}

fn render_title(frame: &mut Frame, app: &App, area: Rect) {
    let mut summary = format!("   {} sets", app.sets.len());
    for state in [State::Open, State::Paused, State::Closed] {
        let n = app.sets.iter().filter(|s| s.state == state).count();
        if n > 0 || state == State::Open {
            summary += &format!(" · {n} {}", state.label());
        }
    }
    let line = Line::from(vec![Span::styled(" rex-workspace", style::heading()), Span::styled(summary, style::muted())]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_sets(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows: Vec<_> = app.matches.iter().map(|m| set_row(&app.sets[m.index], &m.indices)).collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(9),
            Constraint::Length(5),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(header(&["SET", "SESSION", "TABS", "PROJECTS", "DIRTY", "UPDATED"]))
    .block(pane(format!(" Sets {} ", app.matches.len()), app.focus == Focus::Sets, &theme()))
    .highlight_symbol(glyph::POINTER)
    .row_highlight_style(style::selected());
    if app.sets.is_empty() {
        let empty = Paragraph::new("\n  No workspace sets yet. Press n to create one.")
            .style(style::muted())
            .block(pane(" Sets 0 ", true, &theme()));
        frame.render_widget(empty, area);
    } else {
        frame.render_stateful_widget(table, area, &mut app.set_table);
    }
}

fn set_row(set: &SetRow, indices: &[u32]) -> Row<'static> {
    let name_len = set.name.chars().count() as u32;
    let name_hits: Vec<_> = indices.iter().copied().filter(|&i| i < name_len).collect();
    let session = match set.state {
        State::Open => Cell::from(format!("{} open", glyph::OPEN)).style(style::ok()),
        State::Paused => Cell::from(format!("{} paused", glyph::OPEN)).style(style::warn()),
        State::Closed => Cell::from("closed").style(style::muted()),
        State::Idle => placeholder(),
    };
    let tabs = match (set.session.is_some(), set.tabs) {
        (true, Some(n)) => Cell::from(n.to_string()),
        (true, None) => loading(),
        _ => placeholder(),
    };
    let dirty = if set.loading() {
        loading()
    } else if set.dirty() == 0 {
        Cell::from("clean").style(style::ok())
    } else {
        Cell::from(format!("{}/{}", set.dirty(), set.projects.len())).style(style::warn())
    };
    let names: Vec<_> = set.projects.iter().map(|p| p.name.as_str()).collect();
    Row::new(vec![
        Cell::from(Line::from(highlight(&set.name, &name_hits, &theme()))),
        session,
        tabs,
        Cell::from(names.join(" ")),
        dirty,
        Cell::from(ago(set.updated)).style(style::muted()),
    ])
}

fn render_projects(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Projects;
    let Some(set) = app.current() else {
        frame.render_widget(pane(" Projects ", false, &theme()), area);
        return;
    };
    let title = format!(" {} · {} project(s) ", set.name, set.projects.len());
    let area = match set.goal.as_deref() {
        Some(goal) => {
            let [line, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
            let text = Line::from(vec![Span::styled(" Goal ", style::heading()), Span::raw(goal.replace('\n', " "))]);
            frame.render_widget(Paragraph::new(text), line);
            rest
        }
        None => area,
    };
    let rows: Vec<_> = set.projects.iter().map(project_row).collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(22),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(24),
            Constraint::Min(20),
            Constraint::Min(16),
        ],
    )
    .header(header(&["PROJECT", "CHANGE", "STATE", "BOOKMARK", "DESCRIPTION", "SOURCE"]))
    .block(pane(title, focused, &theme()))
    .highlight_symbol(if focused { glyph::POINTER } else { "  " })
    .row_highlight_style(if focused { style::selected() } else { Style::default() });
    if set.projects.is_empty() {
        let empty = Paragraph::new("\n  Empty set. Press a to add repositories.")
            .style(style::muted())
            .block(pane(format!(" {} ", set.name), false, &theme()));
        frame.render_widget(empty, area);
    } else {
        frame.render_stateful_widget(table, area, &mut app.project_table);
    }
}

fn project_row(p: &ProjectRow) -> Row<'static> {
    let (change, state, bookmark, description) = match (&p.status, p.loaded) {
        (Some(s), _) => {
            let state = if s.conflict {
                Cell::from("conflict").style(style::error())
            } else if s.dirty {
                Cell::from(format!("dirty ({})", s.changed_files)).style(style::warn())
            } else {
                Cell::from("clean").style(style::ok())
            };
            let text = |t: &str| if t.is_empty() { placeholder() } else { Cell::from(t.to_string()) };
            (Cell::from(s.change.clone()).style(style::muted()), state, text(&s.bookmarks), text(&s.description))
        }
        (None, false) => (loading(), loading(), placeholder(), placeholder()),
        (None, true) => (placeholder(), Cell::from("missing").style(style::error()), placeholder(), placeholder()),
    };
    Row::new(vec![
        Cell::from(p.name.clone()),
        change,
        state,
        bookmark,
        description,
        Cell::from(tilde(p.source.as_deref().unwrap_or(""))).style(style::muted()),
    ])
}

fn key_hints(app: &App) -> &'static [(&'static str, &'static str)] {
    match (&app.modal, app.mode) {
        (Some(_), _) => &[],
        (None, Mode::Filter) => &[("type", "to filter"), ("↑↓", "move"), ("Enter", "keep"), ("Esc", "clear")],
        (None, Mode::Browse) => &[
            ("↑↓/jk", "move"),
            ("Enter", "open"),
            ("n", "new"),
            ("a", "add"),
            ("x", "remove"),
            ("D", "delete"),
            ("c", "close"),
            ("p", "pause"),
            ("s", "sync"),
            ("e", "goal"),
            ("/", "filter"),
            ("Tab", "focus"),
            ("r", "reload"),
            ("?", "help"),
            ("q", "quit"),
        ],
    }
}

/// The breadcrumb step a modal is on, when it is part of creating a set.
fn new_set_step(modal: &Modal) -> Option<usize> {
    match modal {
        Modal::Input { purpose: Input::NewName, .. } => Some(0),
        Modal::Input { purpose: Input::Version { purpose: Pick::NewRepos(_), .. }, .. }
        | Modal::Pick { purpose: Pick::NewRepos(_), .. } => Some(1),
        _ => None,
    }
}

fn render_modal(frame: &mut Frame, m: &mut Modal, area: Rect) {
    let theme = theme();
    let step = new_set_step(m);
    let wide = area.width.saturating_sub(8).min(72);
    match m {
        Modal::Help => {
            let inner = modal(frame, popup(area, 64, HELP.len() as u16 + 2), "Keys — any key closes", &theme);
            let lines: Vec<_> = HELP
                .iter()
                .map(|(k, d)| Line::from(vec![Span::styled(format!("{k:<14}"), style::heading()), Span::raw(*d)]))
                .collect();
            frame.render_widget(Paragraph::new(lines), inner);
        }
        Modal::Input { title, query, purpose } => {
            let inner = modal(frame, popup(area, wide, 9), title.as_str(), &theme);
            let [crumbs, field, _, keys] =
                Layout::vertical([Constraint::Length(1), Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
                    .areas(inner);
            if let Some(step) = step {
                frame.render_widget(Paragraph::new(steps(NEW_SET, step, &theme)), crumbs);
            }
            query.render(frame, field, "", &theme);
            let back = matches!(purpose, Input::Version { .. });
            let keys_line = if back {
                hints(&[("Enter", "accept"), ("Esc", "back")], &theme)
            } else {
                hints(&[("Enter", "next"), ("Esc", "cancel")], &theme)
            };
            frame.render_widget(Paragraph::new(keys_line), keys);
        }
        Modal::Confirm { message, .. } => {
            let inner = modal(frame, popup(area, wide, 11), "Confirm", &theme);
            let [text, keys] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            frame.render_widget(Paragraph::new(message.as_str()).wrap(Wrap { trim: false }), text);
            frame.render_widget(Paragraph::new(hints(&[("y", "confirm"), ("n/Esc", "cancel")], &theme)), keys);
        }
        Modal::Pick { picker, .. } => {
            let rect = popup(area, area.width.saturating_sub(8).min(100), area.height.saturating_sub(4));
            let mut inner = modal(frame, rect, picker.title.as_str(), &theme);
            if let Some(step) = step {
                let [crumbs, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
                frame.render_widget(Paragraph::new(steps(NEW_SET, step, &theme)), crumbs);
                inner = rest;
            }
            picker.state.render(frame, inner);
        }
    }
}

const HELP: &[(&str, &str)] = &[
    ("↑↓  j k", "move (ctrl+n / ctrl+p also work)"),
    ("g  G", "first / last"),
    ("ctrl+d  ctrl+u", "half page down / up"),
    ("Enter  o", "open the set's session (creates it if needed)"),
    ("n", "new set: name it, then pick repositories (Esc goes back)"),
    ("a", "add repositories to the set"),
    ("x", "remove projects (the highlighted one when focused)"),
    ("D", "delete the set (asks first)"),
    ("c", "close: end the session, freeing everything; open restores its tabs"),
    ("p", "pause / resume: stop its processes (no CPU, memory kept)"),
    ("s", "sync: fetch, then rebase every project onto trunk()"),
    ("e", "edit the set's goal, stated in its AGENTS.md"),
    ("/", "fuzzy-filter sets by name or project"),
    ("Tab", "focus sets / projects"),
    ("r", "reload"),
    ("?", "this help"),
    ("q  Esc", "quit"),
    ("in pickers", "type to filter, Tab marks, Ctrl-F adds a version, Enter accepts"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Env, Manifest, Project};
    use crate::rex::Rex;
    use ratatui::{backend::TestBackend, Terminal};

    fn app_with(dir: &std::path::Path, sets: &[(&str, &[&str])]) -> App {
        let env = Env { workspaces_dir: dir.to_path_buf(), roots: vec![] };
        for (name, projects) in sets {
            let projects = projects.iter().map(|p| Project { name: (*p).into(), source: None, feature: None }).collect();
            env.save(&Manifest { name: (*name).into(), projects, ..Manifest::default() }).unwrap();
        }
        std::env::set_var("REX_BIN", "/nonexistent");
        App::new(env, Rex::locate())
    }

    fn drawn(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        terminal.backend().buffer().content.iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn render_shows_each_set_and_its_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[("edifice", &["console", "reek"])]);
        let text = drawn(&mut app, 120, 30);
        assert!(text.contains("edifice") && text.contains("console") && text.contains("reek"));
    }

    #[test]
    fn render_with_no_sets_invites_creating_one() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[]);
        assert!(drawn(&mut app, 120, 30).contains("Press n to create one"));
    }

    #[test]
    fn render_in_a_tiny_terminal_asks_to_enlarge() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[("a", &[])]);
        assert!(drawn(&mut app, 20, 5).contains("Enlarge"));
    }

    #[test]
    fn render_help_lists_the_open_key() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[("a", &[])]);
        app.modal = Some(Modal::Help);
        assert!(drawn(&mut app, 120, 30).contains("open the set's session"));
    }

    #[test]
    fn render_new_set_name_shows_the_flow_breadcrumb() {
        // GIVEN the first page of creating a set
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[]);
        app.key(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('n')));
        // WHEN
        let text = drawn(&mut app, 120, 30);
        // THEN
        assert!(text.contains("1 Name › 2 Repositories"));
    }

    #[test]
    fn render_shows_a_closed_set_as_closed() {
        // GIVEN
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[("edifice", &["console"])]);
        app.sets[0].state = State::Closed;
        // WHEN / THEN
        assert!(drawn(&mut app, 120, 30).contains("closed"));
    }

    #[test]
    fn render_shows_a_paused_set_as_paused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[("edifice", &["console"])]);
        app.sets[0].state = State::Paused;
        assert!(drawn(&mut app, 120, 30).contains("paused"));
    }

    #[test]
    fn render_with_a_goal_shows_it_above_the_projects() {
        // GIVEN
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app_with(tmp.path(), &[("edifice", &["console"])]);
        app.sets[0].goal = Some("Ship sync".into());
        // WHEN / THEN
        assert!(drawn(&mut app, 120, 30).contains("Goal Ship sync"));
    }

    #[test]
    fn ago_with_no_time_is_a_placeholder() {
        assert_eq!(ago(None), "—");
    }

    #[test]
    fn ago_formats_hours() {
        let t = SystemTime::now() - std::time::Duration::from_secs(7300);
        assert_eq!(ago(Some(t)), "2h ago");
    }
}
