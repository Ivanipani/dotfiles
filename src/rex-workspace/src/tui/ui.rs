//! Rendering: a sets table over a projects table, a fuzzy filter box, and modals.

use super::app::{tilde, App, Focus, Mode, Modal, ProjectRow, SetRow};
use super::picker::{highlight, ACCENT, MUTED};
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Clear, Paragraph, Row, Table},
    Frame,
};
use std::time::SystemTime;
use unicode_width::UnicodeWidthStr;

const MIN_WIDTH: u16 = 50;
const MIN_HEIGHT: u16 = 12;

fn dim() -> Style {
    Style::default().fg(MUTED)
}

fn header(titles: &[&'static str]) -> Row<'static> {
    Row::new(titles.iter().map(|t| Cell::from(*t))).style(Style::default().fg(ACCENT).bold())
}

fn placeholder() -> Cell<'static> {
    Cell::from("—").style(dim())
}

fn ago(time: Option<SystemTime>) -> String {
    let Some(secs) = time.and_then(|t| SystemTime::now().duration_since(t).ok()).map(|d| d.as_secs()) else {
        return "—".into();
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
    let [title, search, sets, projects, status, keys] = Layout::vertical([
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
    render_search(frame, app, search);
    render_sets(frame, app, sets);
    render_projects(frame, app, projects);
    render_status(frame, app, status);
    frame.render_widget(Paragraph::new(hints(app)).style(dim()), keys);
    if let Some(modal) = &mut app.modal {
        render_modal(frame, modal, area);
    }
}

fn render_title(frame: &mut Frame, app: &App, area: Rect) {
    let open = app.sets.iter().filter(|s| s.session.is_some()).count();
    let line = Line::from(vec![
        Span::styled(" rex-workspace", Style::default().fg(ACCENT).bold()),
        Span::styled(format!("   {} sets · {open} open", app.sets.len()), dim()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_search(frame: &mut Frame, app: &App, area: Rect) {
    let filtering = app.mode == Mode::Filter;
    let tail = app.query.tail(usize::from(area.width.saturating_sub(5)));
    let body = if tail.is_empty() && !filtering {
        Line::from(Span::styled("  press / to filter sets and projects", dim()))
    } else {
        Line::from(format!("> {tail}"))
    };
    let count = format!(" {}/{} ", app.matches.len(), app.sets.len());
    let color = if filtering { ACCENT } else { MUTED };
    frame.render_widget(
        Paragraph::new(body).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(color))
                .title(Span::styled(count, dim())),
        ),
        area,
    );
    if filtering && app.modal.is_none() {
        frame.set_cursor_position((area.x + 3 + UnicodeWidthStr::width(tail) as u16, area.y + 1));
    }
}

fn pane(title: String, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { ACCENT } else { MUTED }))
        .title(Span::styled(title, Style::default().fg(if focused { ACCENT } else { Color::Reset }).bold()))
}

fn selected_style() -> Style {
    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
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
    .block(pane(format!(" Sets {} ", app.matches.len()), app.focus == Focus::Sets))
    .highlight_symbol("› ")
    .row_highlight_style(selected_style());
    if app.sets.is_empty() {
        let empty = Paragraph::new("\n  No workspace sets yet. Press n to create one.").style(dim()).block(pane(" Sets 0 ".into(), true));
        frame.render_widget(empty, area);
    } else {
        frame.render_stateful_widget(table, area, &mut app.set_table);
    }
}

fn set_row(set: &SetRow, indices: &[u32]) -> Row<'static> {
    let name_len = set.name.chars().count() as u32;
    let name_hits: Vec<_> = indices.iter().copied().filter(|&i| i < name_len).collect();
    let session = if set.session.is_some() {
        Cell::from("● open").style(Style::default().fg(Color::Green))
    } else {
        placeholder()
    };
    let tabs = match (set.session.is_some(), set.tabs) {
        (true, Some(n)) => Cell::from(n.to_string()),
        (true, None) => Cell::from("…").style(dim()),
        _ => placeholder(),
    };
    let dirty = if set.loading() {
        Cell::from("…").style(dim())
    } else if set.dirty() == 0 {
        Cell::from("clean").style(Style::default().fg(Color::Green))
    } else {
        Cell::from(format!("{}/{}", set.dirty(), set.projects.len())).style(Style::default().fg(Color::Yellow))
    };
    let names: Vec<_> = set.projects.iter().map(|p| p.name.as_str()).collect();
    Row::new(vec![
        Cell::from(Line::from(highlight(&set.name, &name_hits))),
        session,
        tabs,
        Cell::from(names.join(" ")),
        dirty,
        Cell::from(ago(set.updated)).style(dim()),
    ])
}

fn render_projects(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Projects;
    let Some(set) = app.current() else {
        frame.render_widget(Block::bordered().border_type(BorderType::Rounded).border_style(dim()).title(" Projects "), area);
        return;
    };
    let title = format!(" {} · {} project(s) ", set.name, set.projects.len());
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
    .block(pane(title, focused))
    .highlight_symbol(if focused { "› " } else { "  " })
    .row_highlight_style(if focused { selected_style() } else { Style::default() });
    if set.projects.is_empty() {
        let empty = Paragraph::new("\n  Empty set. Press a to add repositories.").style(dim()).block(pane(format!(" {} ", set.name), false));
        frame.render_widget(empty, area);
    } else {
        frame.render_stateful_widget(table, area, &mut app.project_table);
    }
}

fn project_row(p: &ProjectRow) -> Row<'static> {
    let (change, state, bookmark, description) = match (&p.status, p.loaded) {
        (Some(s), _) => {
            let state = if s.dirty {
                Cell::from(format!("dirty ({})", s.changed_files)).style(Style::default().fg(Color::Yellow))
            } else {
                Cell::from("clean").style(Style::default().fg(Color::Green))
            };
            let text = |t: &str| if t.is_empty() { placeholder() } else { Cell::from(t.to_string()) };
            (Cell::from(s.change.clone()).style(dim()), state, text(&s.bookmarks), text(&s.description))
        }
        (None, false) => (Cell::from("…").style(dim()), Cell::from("…").style(dim()), placeholder(), placeholder()),
        (None, true) => (placeholder(), Cell::from("missing").style(Style::default().fg(Color::Red)), placeholder(), placeholder()),
    };
    Row::new(vec![
        Cell::from(p.name.clone()),
        change,
        state,
        bookmark,
        description,
        Cell::from(tilde(p.source.as_deref().unwrap_or(""))).style(dim()),
    ])
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.status {
        Some(s) if s.error => Line::from(Span::styled(format!(" ✗ {}", s.text), Style::default().fg(Color::Red))),
        Some(s) => Line::from(Span::styled(format!(" ✓ {}", s.text), Style::default().fg(Color::Green))),
        None => Line::default(),
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn hints(app: &App) -> &'static str {
    match (&app.modal, app.mode) {
        (Some(_), _) => " Esc cancel",
        (None, Mode::Filter) => " type to filter  ↑↓ move  Enter keep  Esc clear",
        (None, Mode::Browse) => " ↑↓/jk move  Enter open  n new  a add  x remove  D delete  / filter  Tab focus  r reload  ? help  q quit",
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [v] = Layout::vertical([Constraint::Length(height.min(area.height))]).flex(Flex::Center).areas(area);
    let [h] = Layout::horizontal([Constraint::Length(width.min(area.width))]).flex(Flex::Center).areas(v);
    h
}

fn modal_block(title: &str) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(title.to_string(), Style::default().fg(ACCENT).bold()))
}

fn render_modal(frame: &mut Frame, modal: &mut Modal, area: Rect) {
    match modal {
        Modal::Help => {
            let rect = centered(area, 64, HELP.len() as u16 + 2);
            frame.render_widget(Clear, rect);
            let lines: Vec<_> = HELP
                .iter()
                .map(|(k, d)| Line::from(vec![Span::styled(format!(" {k:<14}"), Style::default().fg(ACCENT).bold()), Span::raw(*d)]))
                .collect();
            frame.render_widget(Paragraph::new(lines).block(modal_block(" Keys — any key closes ")), rect);
        }
        Modal::Input { title, query, .. } => {
            let rect = centered(area, 50, 3);
            frame.render_widget(Clear, rect);
            let tail = query.tail(usize::from(rect.width.saturating_sub(5))).to_string();
            frame.render_widget(Paragraph::new(format!("> {tail}")).block(modal_block(title)), rect);
            frame.set_cursor_position((rect.x + 3 + UnicodeWidthStr::width(tail.as_str()) as u16, rect.y + 1));
        }
        Modal::Confirm { message, .. } => {
            let rect = centered(area, 64, 5);
            frame.render_widget(Clear, rect);
            let body = vec![Line::raw(format!(" {message}")), Line::raw(""), Line::from(Span::styled(" y confirm   n/Esc cancel", dim()))];
            frame.render_widget(Paragraph::new(body).wrap(ratatui::widgets::Wrap { trim: false }).block(modal_block(" Confirm ")), rect);
        }
        Modal::Pick { picker, .. } => {
            let rect = centered(area, area.width.saturating_sub(8).min(100), area.height.saturating_sub(4));
            frame.render_widget(Clear, rect);
            let block = modal_block(&picker.title);
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            picker.render(frame, inner);
        }
    }
}

const HELP: &[(&str, &str)] = &[
    ("↑↓  j k", "move (ctrl+n / ctrl+p also work)"),
    ("g  G", "first / last"),
    ("ctrl+d  ctrl+u", "half page down / up"),
    ("Enter  o", "open the set's session (creates it if needed)"),
    ("n", "new set: name it, then pick repositories"),
    ("a", "add repositories to the set"),
    ("x", "remove projects (the highlighted one when focused)"),
    ("D", "delete the set (asks first)"),
    ("/", "fuzzy-filter sets by name or project"),
    ("Tab", "focus sets / projects"),
    ("r", "reload"),
    ("?", "this help"),
    ("q  Esc", "quit"),
    ("in pickers", "type to filter, Tab marks, Enter accepts"),
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
            let projects = projects.iter().map(|p| Project { name: (*p).into(), source: None }).collect();
            env.save(&Manifest { name: (*name).into(), projects }).unwrap();
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
    fn ago_with_no_time_is_a_placeholder() {
        assert_eq!(ago(None), "—");
    }

    #[test]
    fn ago_formats_hours() {
        let t = SystemTime::now() - std::time::Duration::from_secs(7300);
        assert_eq!(ago(Some(t)), "2h ago");
    }
}
