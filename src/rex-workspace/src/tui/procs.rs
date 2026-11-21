//! Full-screen process browser: what runs under your Rex panes, and a way to stop it.

use super::app::Status;
use claw::style::{self, glyph, Theme};
use claw::widgets::{highlight, hints, modal, pane, popup, status, TextInput};
use claw::{single_line, FuzzyFinder, Match};
use crate::procs::{self, mem, Host, Row, Snapshot, Usage};
use crate::rex::{Rex, HUB_LABEL, TOGGLE_ACTION};
use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row as TableRow, Table, TableState, Wrap},
    Frame,
};
use std::collections::BTreeSet;
use std::sync::mpsc;
use std::time::Duration;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 11;
/// Processes change quickly; re-read them in the background this often.
const REFRESH: Duration = Duration::from_secs(2);
const BIG: u64 = 2 * 1_048_576;
const MEDIUM: u64 = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Filter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Mem,
    Cpu,
}

pub struct Confirm {
    pub signal: &'static str,
    pub pids: Vec<u32>,
    pub message: String,
}

pub struct App {
    rex: Rex,
    pub hub: bool,
    pub rows: Vec<Row>,
    /// What every process on the system uses, and the machine's capacity.
    pub all: Usage,
    pub host: Option<Host>,
    finder: FuzzyFinder,
    pub query: TextInput,
    pub matches: Vec<Match>,
    pub selected: usize,
    pub marked: BTreeSet<u32>,
    pub mode: Mode,
    pub sort: Sort,
    pub confirm: Option<Confirm>,
    pub status: Option<Status>,
    pub table: TableState,
    page: usize,
    pub quit: bool,
    /// Set when a refresh should happen now rather than at the next interval.
    kick: bool,
}

/// What the fuzzy filter matches on: session, pid and command.
fn label(row: &Row) -> String {
    single_line(&format!("{} {} {}", row.session, row.proc.pid, row.proc.args))
}

impl App {
    pub fn new(rex: Rex) -> Self {
        let hub = rex.current_label().as_deref() == Some(HUB_LABEL);
        Self {
            rex,
            hub,
            rows: vec![],
            all: Usage::default(),
            host: None,
            finder: FuzzyFinder::new(Vec::<String>::new()),
            query: TextInput::default(),
            matches: vec![],
            selected: 0,
            marked: BTreeSet::new(),
            mode: Mode::Browse,
            sort: Sort::Mem,
            confirm: None,
            status: None,
            table: TableState::default(),
            page: 10,
            quit: false,
            kick: false,
        }
    }

    pub fn current(&self) -> Option<&Row> {
        self.rows.get(self.matches.get(self.selected)?.index)
    }

    pub fn set_snapshot(&mut self, snap: Snapshot) {
        self.all = snap.all;
        self.host = snap.host;
        self.set_rows(snap.rows);
    }

    /// Replace the process list, keeping the cursor on the same pid and dropping stale marks.
    pub fn set_rows(&mut self, mut rows: Vec<Row>) {
        let keep = self.current().map(|r| r.proc.pid);
        sort_rows(&mut rows, self.sort);
        self.rows = rows;
        self.marked.retain(|pid| self.rows.iter().any(|r| r.proc.pid == *pid));
        self.finder = FuzzyFinder::new(self.rows.iter().map(label));
        self.refilter();
        if let Some(i) = keep.and_then(|pid| self.matches.iter().position(|m| self.rows[m.index].proc.pid == pid)) {
            self.selected = i;
        }
    }

    fn refilter(&mut self) {
        self.matches = self.finder.search(self.query.text());
        self.selected = self.selected.min(self.matches.len().saturating_sub(1));
    }

    fn resort(&mut self, sort: Sort) {
        self.sort = sort;
        let rows = self.rows.clone();
        self.set_rows(rows);
    }

    fn move_by(&mut self, amount: isize) {
        self.selected = self.selected.saturating_add_signed(amount).min(self.matches.len().saturating_sub(1));
    }

    /// The marked processes, else the highlighted one.
    fn targets(&self) -> Vec<&Row> {
        if self.marked.is_empty() {
            return self.current().into_iter().collect();
        }
        self.rows.iter().filter(|r| self.marked.contains(&r.proc.pid)).collect()
    }

    fn ask(&mut self, signal: &'static str) {
        let targets = self.targets();
        if targets.is_empty() {
            return self.fail("no process selected");
        }
        let freed: u64 = targets.iter().map(|r| r.proc.rss_kb).sum();
        let what = match targets.as_slice() {
            [one] => format!("{} ({})", single_line(&one.proc.args), one.proc.pid),
            many => format!("{} processes", many.len()),
        };
        let verb = if signal == "KILL" { "Force kill" } else { "Stop" };
        let message = format!("{verb} {what}?\n\nResident memory: {}", mem(freed));
        let pids = targets.iter().map(|r| r.proc.pid).collect();
        self.confirm = Some(Confirm { signal, pids, message });
    }

    fn send(&mut self, confirm: Confirm) {
        let failures: Vec<_> = confirm.pids.iter().filter_map(|&pid| procs::signal(confirm.signal, pid).err()).collect();
        self.marked.clear();
        self.kick = true;
        match failures.first() {
            Some(e) => self.fail(format!("{e:#}")),
            None => self.status = Some(Status { text: format!("sent {} to {} process(es)", confirm.signal, confirm.pids.len()), error: false }),
        }
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.status = Some(Status { text: text.into(), error: true });
    }

    /// Quit, or from the shared hub session go back to where the user came from.
    fn leave(&mut self) {
        if !self.hub {
            self.quit = true;
        } else if let Err(e) = self.rex.perform(&[TOGGLE_ACTION]) {
            self.fail(format!("could not leave the rex-manager session: {e:#}"));
        }
    }

    pub fn key(&mut self, key: KeyEvent) {
        if let Some(confirm) = self.confirm.take() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => self.send(confirm),
                KeyCode::Char('n') | KeyCode::Esc => {}
                _ => self.confirm = Some(confirm),
            }
            return;
        }
        self.status = None;
        match self.mode {
            Mode::Filter => self.filter_key(key),
            Mode::Browse => self.browse_key(key),
        }
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.mode = Mode::Browse,
            KeyCode::Esc => {
                self.query.clear();
                self.refilter();
                self.mode = Mode::Browse;
            }
            KeyCode::Down => self.move_by(1),
            KeyCode::Up => self.move_by(-1),
            _ => {
                if self.query.edit(key) {
                    self.selected = 0;
                    self.refilter();
                }
            }
        }
    }

    fn browse_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') => self.leave(),
            KeyCode::Esc if !self.query.text().is_empty() => {
                self.query.clear();
                self.refilter();
            }
            KeyCode::Esc => self.leave(),
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Char('n') if ctrl => self.move_by(1),
            KeyCode::Char('p') if ctrl => self.move_by(-1),
            KeyCode::Char('d') if ctrl => self.move_by((self.page / 2).max(1) as isize),
            KeyCode::Char('u') if ctrl => self.move_by(-((self.page / 2).max(1) as isize)),
            KeyCode::PageDown => self.move_by(self.page as isize),
            KeyCode::PageUp => self.move_by(-(self.page as isize)),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.selected = self.matches.len().saturating_sub(1),
            KeyCode::Char('/') => self.mode = Mode::Filter,
            KeyCode::Tab | KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('t') => self.ask("TERM"),
            KeyCode::Char('K') => self.ask("KILL"),
            KeyCode::Char('m') => self.resort(Sort::Mem),
            KeyCode::Char('c') => self.resort(Sort::Cpu),
            KeyCode::Char('r') => self.kick = true,
            _ => {}
        }
    }

    fn toggle_mark(&mut self) {
        if let Some(pid) = self.current().map(|r| r.proc.pid) {
            if !self.marked.remove(&pid) {
                self.marked.insert(pid);
            }
            self.move_by(1);
        }
    }

    /// Whether a refresh was requested since the last call.
    pub fn take_kick(&mut self) -> bool {
        std::mem::take(&mut self.kick)
    }
}

fn sort_rows(rows: &mut [Row], sort: Sort) {
    match sort {
        Sort::Mem => rows.sort_by(|a, b| b.proc.rss_kb.cmp(&a.proc.rss_kb)),
        Sort::Cpu => rows.sort_by(|a, b| b.proc.cpu.total_cmp(&a.proc.cpu)),
    }
}

// --- run loop ---------------------------------------------------------------

/// Browse until quit. `session` limits the view to one Rex session by label.
pub fn run(rex: Rex, session: Option<String>) -> Result<()> {
    if let claw::TerminalStatus::Unavailable(reason) = claw::TerminalStatus::detect() {
        bail!("the interactive view needs a terminal ({reason}); use --list for a table");
    }
    let (rows_tx, rows_rx) = mpsc::channel();
    let (kick_tx, kick_rx) = mpsc::channel::<()>();
    let worker = rex.clone();
    std::thread::spawn(move || loop {
        let rows = procs::collect(&worker, session.as_deref());
        if rows_tx.send(rows).is_err() || matches!(kick_rx.recv_timeout(REFRESH), Err(mpsc::RecvTimeoutError::Disconnected)) {
            break;
        }
    });
    let mut app = App::new(rex);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app, &rows_rx, &kick_tx);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rows: &mpsc::Receiver<Result<Snapshot>>,
    kick: &mpsc::Sender<()>,
) -> Result<()> {
    while !app.quit {
        while let Ok(latest) = rows.try_recv() {
            match latest {
                Ok(snap) => app.set_snapshot(snap),
                Err(e) => app.status = Some(Status { text: format!("{e:#}"), error: true }),
            }
        }
        terminal.draw(|f| render(f, app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    app.key(key);
                }
            }
        }
        if app.take_kick() {
            let _ = kick.send(());
        }
    }
    Ok(())
}

// --- rendering --------------------------------------------------------------

fn theme() -> Theme {
    Theme::default()
}

fn mem_style(kb: u64) -> Style {
    match kb {
        k if k >= BIG => style::error(),
        k if k >= MEDIUM => style::warn(),
        _ => Style::default(),
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(Paragraph::new("Enlarge the pane\nq to quit"), area);
        return;
    }
    let [title, search, list, detail, status_area, keys] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    app.page = usize::from(list.height.saturating_sub(3)).max(1);
    render_title(frame, app, title);
    let count = format!(" {}/{} ", app.matches.len(), app.rows.len());
    let filtering = app.mode == Mode::Filter && app.confirm.is_none();
    app.query.search(frame, search, &count, filtering, "press / to filter by session, pid or command", &theme());
    render_table(frame, app, list);
    render_detail(frame, app, detail);
    let line = app.status.as_ref().map_or_else(Line::default, |s| status(&s.text, s.error, &theme()));
    frame.render_widget(Paragraph::new(line), status_area);
    frame.render_widget(Paragraph::new(hints(key_hints(app), &theme())), keys);
    if let Some(confirm) = &app.confirm {
        render_confirm(frame, confirm, area);
    }
}

fn render_title(frame: &mut Frame, app: &App, area: Rect) {
    let marked = if app.marked.is_empty() { String::new() } else { format!(" · {} marked", app.marked.len()) };
    let title = Line::from(vec![
        Span::styled(" rex-workspace procs", style::heading()),
        Span::styled(format!("   {} processes{marked}", app.rows.len()), style::muted()),
    ]);
    let [cpu, mem] = procs::summary(Usage::of(app.rows.iter().map(|r| &r.proc)), app.all, app.host.as_ref());
    let usage = Line::from(Span::styled(format!(" {cpu}  │  {mem}"), style::muted()));
    frame.render_widget(Paragraph::new(vec![title, usage]), area);
}

fn render_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let sort = match app.sort {
        Sort::Mem => "memory",
        Sort::Cpu => "cpu",
    };
    let title = format!(" Processes · by {sort} ");
    if app.matches.is_empty() {
        let text = if app.rows.is_empty() { "\n  Looking for processes under your Rex panes…" } else { "\n  No matches. Esc clears the filter." };
        frame.render_widget(Paragraph::new(text).style(style::muted()).block(pane(title, true, &theme())), area);
        return;
    }
    let rows: Vec<_> = app.matches.iter().map(|m| table_row(&app.rows[m.index], &m.indices, app.marked.contains(&app.rows[m.index].proc.pid))).collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(20),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(11),
            Constraint::Min(20),
        ],
    )
    .header(TableRow::new(["", "SESSION", "PID", "MEM", "CPU%", "TIME", "COMMAND"].map(Cell::from)).style(style::heading()))
    .block(pane(title, true, &theme()))
    .highlight_symbol(glyph::POINTER)
    .row_highlight_style(style::selected());
    app.table.select(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut app.table);
}

fn table_row(row: &Row, indices: &[u32], marked: bool) -> TableRow<'static> {
    let session_len = row.session.chars().count() as u32;
    let pid_len = row.proc.pid.to_string().chars().count() as u32;
    let command_start = session_len + 1 + pid_len + 1;
    let session_hits: Vec<_> = indices.iter().copied().filter(|&i| i < session_len).collect();
    let command_hits: Vec<_> = indices.iter().filter_map(|&i| i.checked_sub(command_start)).collect();
    let cpu = if row.proc.cpu >= 50.0 { style::warn() } else { Style::default() };
    TableRow::new(vec![
        Cell::from(if marked { glyph::OPEN } else { "" }).style(style::accent()),
        Cell::from(Line::from(highlight(&single_line(&row.session), &session_hits, &theme()))),
        Cell::from(row.proc.pid.to_string()).style(style::muted()),
        Cell::from(mem(row.proc.rss_kb)).style(mem_style(row.proc.rss_kb)),
        Cell::from(format!("{:.1}", row.proc.cpu)).style(cpu),
        Cell::from(row.proc.etime.clone()).style(style::muted()),
        Cell::from(Line::from(highlight(&single_line(&row.proc.args), &command_hits, &theme()))),
    ])
}

fn render_detail(frame: &mut Frame, app: &App, area: Rect) {
    let text = app.current().map(|r| format!(" {} · ppid {} · {}", r.session, r.proc.ppid, single_line(&r.proc.args))).unwrap_or_default();
    frame.render_widget(Paragraph::new(text).style(style::muted()), area);
}

fn key_hints(app: &App) -> &'static [(&'static str, &'static str)] {
    match (&app.confirm, app.mode) {
        (Some(_), _) => &[("y", "confirm"), ("n/Esc", "cancel")],
        (None, Mode::Filter) => &[("type", "to filter"), ("↑↓", "move"), ("Enter", "keep"), ("Esc", "clear")],
        (None, Mode::Browse) => &[
            ("↑↓/jk", "move"),
            ("Tab", "mark"),
            ("t", "stop"),
            ("K", "force kill"),
            ("/", "filter"),
            ("m", "mem"),
            ("c", "cpu"),
            ("r", "refresh"),
            ("q", "quit"),
        ],
    }
}

fn render_confirm(frame: &mut Frame, confirm: &Confirm, area: Rect) {
    let theme = theme();
    let inner = modal(frame, popup(area, area.width.saturating_sub(8).min(72), 9), "Confirm", &theme);
    let [text, keys] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    frame.render_widget(Paragraph::new(confirm.message.as_str()).wrap(Wrap { trim: false }), text);
    frame.render_widget(Paragraph::new(hints(&[("y", "confirm"), ("n/Esc", "cancel")], &theme)), keys);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procs::Proc;
    use ratatui::{backend::TestBackend, Terminal};

    fn row(pid: u32, session: &str, rss_kb: u64, cpu: f32, args: &str) -> Row {
        Row { proc: Proc { pid, ppid: 1, rss_kb, cpu, etime: "01:00".into(), args: args.into() }, session: session.into() }
    }

    fn app() -> App {
        std::env::set_var("REX_BIN", "/nonexistent");
        let mut app = App::new(Rex::locate());
        app.set_rows(vec![row(1, "work", 4096, 1.0, "nvim ."), row(2, "work", 2_097_152, 90.0, "rust-analyzer"), row(3, "play", 100, 0.0, "zsh")]);
        app
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn drawn(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        terminal.backend().buffer().content.iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn set_rows_sorts_the_biggest_memory_first() {
        assert_eq!(app().current().unwrap().proc.pid, 2);
    }

    #[test]
    fn set_rows_keeps_the_cursor_on_the_same_pid() {
        // GIVEN the cursor on the second row (pid 1)
        let mut app = app();
        app.key(key(KeyCode::Down));
        // WHEN the list refreshes with a new biggest process
        app.set_rows(vec![row(9, "x", 9_000_000, 0.0, "big"), row(1, "work", 4096, 1.0, "nvim ."), row(2, "work", 100, 0.0, "ra")]);
        // THEN the cursor still points at pid 1
        assert_eq!(app.current().unwrap().proc.pid, 1);
    }

    #[test]
    fn set_rows_drops_marks_for_processes_that_exited() {
        let mut app = app();
        app.key(key(KeyCode::Tab));
        app.set_rows(vec![row(1, "work", 4096, 1.0, "nvim .")]);
        assert!(app.marked.is_empty());
    }

    #[test]
    fn tab_marks_the_highlighted_process_and_moves_down() {
        let mut app = app();
        app.key(key(KeyCode::Tab));
        assert_eq!((app.marked.contains(&2), app.current().unwrap().proc.pid), (true, 1));
    }

    #[test]
    fn t_without_marks_asks_to_stop_the_highlighted_process() {
        let mut app = app();
        app.key(key(KeyCode::Char('t')));
        assert_eq!(app.confirm.as_ref().map(|c| (c.signal, c.pids.clone())), Some(("TERM", vec![2])));
    }

    #[test]
    fn shift_k_asks_for_a_force_kill() {
        let mut app = app();
        app.key(key(KeyCode::Char('K')));
        assert_eq!(app.confirm.unwrap().signal, "KILL");
    }

    #[test]
    fn t_with_marks_targets_every_marked_process() {
        let mut app = app();
        app.key(key(KeyCode::Tab));
        app.key(key(KeyCode::Tab));
        app.key(key(KeyCode::Char('t')));
        let mut pids = app.confirm.unwrap().pids;
        pids.sort();
        assert_eq!(pids, vec![1, 2]);
    }

    #[test]
    fn n_cancels_the_confirmation_without_signalling() {
        let mut app = app();
        app.key(key(KeyCode::Char('t')));
        app.key(key(KeyCode::Char('n')));
        assert!(app.confirm.is_none() && app.status.is_none());
    }

    #[test]
    fn filter_narrows_matches_to_the_command() {
        let mut app = app();
        app.key(key(KeyCode::Char('/')));
        "zsh".chars().for_each(|c| app.key(key(KeyCode::Char(c))));
        assert_eq!(app.current().unwrap().proc.pid, 3);
    }

    #[test]
    fn c_sorts_by_cpu_busiest_first() {
        let mut app = app();
        app.set_rows(vec![row(1, "a", 9, 5.0, "a"), row(2, "a", 8, 50.0, "b")]);
        app.key(key(KeyCode::Char('c')));
        assert_eq!(app.rows[0].proc.pid, 2);
    }

    #[test]
    fn r_requests_an_immediate_refresh() {
        let mut app = app();
        app.key(key(KeyCode::Char('r')));
        assert!(app.take_kick());
    }

    #[test]
    fn q_outside_the_hub_quits() {
        let mut app = app();
        app.key(key(KeyCode::Char('q')));
        assert!(app.quit);
    }

    #[test]
    fn render_shows_memory_and_commands() {
        let text = drawn(&mut app(), 120, 24);
        assert!(text.contains("rust-analyzer") && text.contains("2.0G"));
    }

    #[test]
    fn render_shows_the_share_of_system_cpu_under_rex() {
        // GIVEN rex rows using 91% cpu out of 182% system-wide
        let mut app = app();
        let rows = app.rows.clone();
        app.set_snapshot(Snapshot { rows, all: Usage { cpu: 182.0, rss_kb: 1 }, host: None });
        // WHEN / THEN
        assert!(drawn(&mut app, 160, 24).contains("50% of all use"));
    }

    #[test]
    fn render_in_a_tiny_terminal_asks_to_enlarge() {
        assert!(drawn(&mut app(), 20, 5).contains("Enlarge"));
    }

    #[test]
    fn render_confirm_names_the_target() {
        let mut app = app();
        app.key(key(KeyCode::Char('t')));
        assert!(drawn(&mut app, 120, 24).contains("rust-analyzer (2)"));
    }
}
