//! A fuzzy picker modal: type to filter, tab to mark (multi), enter to accept.

use super::query::{single_line, Query};
use crate::finder::{FuzzyFinder, Match};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::collections::BTreeSet;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const ACCENT: Color = Color::Cyan;
pub const MUTED: Color = Color::DarkGray;
pub const MATCH: Color = Color::Yellow;

pub struct Choice {
    pub label: String,
    pub detail: String,
}

impl Choice {
    pub fn new(label: &str, detail: &str) -> Self {
        Self { label: single_line(label), detail: single_line(detail) }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Cancel,
    /// Indices into the original choices, ascending.
    Accept(Vec<usize>),
    /// Ctrl-F on a choice: the caller names an extra version of it, then calls `add_marked`.
    Version(usize),
}

pub struct Picker {
    pub title: String,
    choices: Vec<Choice>,
    finder: FuzzyFinder,
    query: Query,
    matches: Vec<Match>,
    selected: usize,
    marked: BTreeSet<usize>,
    multi: bool,
    list: ListState,
    page: usize,
}

impl Picker {
    pub fn new(title: impl Into<String>, choices: Vec<Choice>, multi: bool) -> Self {
        let finder = FuzzyFinder::new(choices.iter().map(|c| c.label.as_str()));
        let mut picker = Self {
            title: title.into(),
            choices,
            finder,
            query: Query::default(),
            matches: vec![],
            selected: 0,
            marked: BTreeSet::new(),
            multi,
            list: ListState::default(),
            page: 10,
        };
        picker.refresh();
        picker
    }

    fn refresh(&mut self) {
        self.matches = self.finder.search(&self.query.text);
        self.selected = 0;
    }

    fn move_by(&mut self, amount: isize) {
        self.selected = self.selected.saturating_add_signed(amount).min(self.matches.len().saturating_sub(1));
    }

    /// Appends a choice, marked and ready to accept; the filter is cleared so it is visible.
    pub fn add_marked(&mut self, choice: Choice) {
        self.choices.push(choice);
        self.finder = FuzzyFinder::new(self.choices.iter().map(|c| c.label.as_str()));
        self.marked.insert(self.choices.len() - 1);
        self.query = Query::default();
        self.refresh();
        self.selected = self.choices.len() - 1;
    }

    pub fn key(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('f') if ctrl && self.multi => {
                if let Some(m) = self.matches.get(self.selected) {
                    return Outcome::Version(m.index);
                }
            }
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('c') if ctrl => return Outcome::Cancel,
            KeyCode::Enter => {
                if !self.marked.is_empty() {
                    return Outcome::Accept(self.marked.iter().copied().collect());
                }
                if let Some(m) = self.matches.get(self.selected) {
                    return Outcome::Accept(vec![m.index]);
                }
            }
            KeyCode::Down => self.move_by(1),
            KeyCode::Up => self.move_by(-1),
            KeyCode::Char('n') if ctrl => self.move_by(1),
            KeyCode::Char('p') if ctrl => self.move_by(-1),
            KeyCode::PageDown => self.move_by(self.page as isize),
            KeyCode::PageUp => self.move_by(-(self.page as isize)),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.matches.len().saturating_sub(1),
            KeyCode::Tab if self.multi => {
                if let Some(m) = self.matches.get(self.selected) {
                    if !self.marked.remove(&m.index) {
                        self.marked.insert(m.index);
                    }
                    self.move_by(1);
                }
            }
            _ => {
                if self.query.edit(key) {
                    self.refresh();
                }
            }
        }
        Outcome::Continue
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let [search, results, detail, help] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
                .areas(area);
        let tail = self.query.tail(usize::from(search.width.saturating_sub(5))).to_string();
        let count = format!(" {}/{}{} ", self.matches.len(), self.choices.len(), if self.multi { format!(" · {} marked", self.marked.len()) } else { String::new() });
        frame.render_widget(
            Paragraph::new(format!("> {tail}")).block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(ACCENT))
                    .title(Span::styled(count, Style::default().fg(MUTED))),
            ),
            search,
        );
        frame.set_cursor_position((search.x + 3 + UnicodeWidthStr::width(tail.as_str()) as u16, search.y + 1));

        if self.matches.is_empty() {
            frame.render_widget(
                Paragraph::new("  No matches. Edit your search or Ctrl-U to clear.").style(Style::default().fg(MUTED)),
                results,
            );
        } else {
            let items: Vec<_> = self.matches.iter().map(|m| self.item(m)).collect();
            self.list.select(Some(self.selected));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_symbol("› ")
                    .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)),
                results,
                &mut self.list,
            );
        }
        let detail_text = self.matches.get(self.selected).map(|m| self.choices[m.index].detail.as_str()).unwrap_or("");
        frame.render_widget(Paragraph::new(format!("  {detail_text}")).style(Style::default().fg(MUTED)), detail);
        let keys = if self.multi { " ↑↓ move  Tab mark  ^f version  Enter accept  Esc cancel" } else { " ↑↓ move  Enter accept  Esc cancel" };
        frame.render_widget(Paragraph::new(keys).style(Style::default().fg(MUTED)), help);
        self.page = usize::from(results.height).max(1);
    }

    fn item(&self, m: &Match) -> ListItem<'static> {
        let marker = match (self.multi, self.marked.contains(&m.index)) {
            (false, _) => "",
            (true, true) => "[x] ",
            (true, false) => "[ ] ",
        };
        let mut spans = vec![Span::styled(marker, Style::default().fg(ACCENT))];
        spans.extend(highlight(&self.choices[m.index].label, &m.indices));
        ListItem::new(Line::from(spans))
    }
}

/// Spans for `text` with the characters at `indices` (scalar offsets) emphasized.
pub fn highlight(text: &str, indices: &[u32]) -> Vec<Span<'static>> {
    let mut scalar = 0u32;
    text.graphemes(true)
        .map(|g| {
            let end = scalar + g.chars().count() as u32;
            let hit = indices.iter().any(|&i| (scalar..end).contains(&i));
            scalar = end;
            let style = if hit { Style::default().fg(MATCH).bold() } else { Style::default() };
            Span::styled(g.to_string(), style)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn picker(multi: bool) -> Picker {
        Picker::new("pick", ["alpha", "beta", "beta"].map(|l| Choice::new(l, "")).into(), multi)
    }

    #[test]
    fn enter_accepts_the_focused_choice() {
        let mut p = picker(false);
        p.key(key(KeyCode::End));
        assert_eq!(p.key(key(KeyCode::Enter)), Outcome::Accept(vec![2]));
    }

    #[test]
    fn enter_with_no_matches_keeps_the_picker_open() {
        let mut p = picker(false);
        p.key(key(KeyCode::Char('z')));
        assert_eq!(p.key(key(KeyCode::Enter)), Outcome::Continue);
    }

    #[test]
    fn esc_cancels() {
        assert_eq!(picker(false).key(key(KeyCode::Esc)), Outcome::Cancel);
    }

    #[test]
    fn marks_survive_filtering_and_keep_duplicate_identity() {
        // GIVEN the last and first choices marked
        let mut p = picker(true);
        p.key(key(KeyCode::End));
        p.key(key(KeyCode::Tab));
        p.key(key(KeyCode::Home));
        p.key(key(KeyCode::Tab));
        // WHEN a filter hides every choice
        p.key(key(KeyCode::Char('z')));
        // THEN both marks are still accepted, in input order
        assert_eq!(p.key(key(KeyCode::Enter)), Outcome::Accept(vec![0, 2]));
    }

    #[test]
    fn ctrl_f_asks_for_a_version_of_the_focused_choice() {
        // GIVEN the second choice focused
        let mut p = picker(true);
        p.key(key(KeyCode::Down));
        // WHEN
        let outcome = p.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        // THEN
        assert_eq!(outcome, Outcome::Version(1));
    }

    #[test]
    fn ctrl_f_in_single_mode_does_nothing() {
        let mut p = picker(false);
        assert_eq!(p.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)), Outcome::Continue);
    }

    #[test]
    fn add_marked_makes_the_new_choice_accepted_with_the_marked_ones() {
        // GIVEN the first choice marked
        let mut p = picker(true);
        p.key(key(KeyCode::Tab));
        // WHEN a version is added
        p.add_marked(Choice::new("alpha-v2", ""));
        // THEN accepting returns both
        assert_eq!(p.key(key(KeyCode::Enter)), Outcome::Accept(vec![0, 3]));
    }

    #[test]
    fn tab_in_single_mode_marks_nothing() {
        let mut p = picker(false);
        p.key(key(KeyCode::Tab));
        assert_eq!(p.key(key(KeyCode::Enter)), Outcome::Accept(vec![0]));
    }

    #[test]
    fn render_when_nothing_matches_says_so() {
        // GIVEN
        let mut p = picker(false);
        p.key(key(KeyCode::Char('z')));
        let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
        // WHEN
        terminal.draw(|f| p.render(f, f.area())).unwrap();
        // THEN
        let text: String = terminal.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("No matches"));
    }

    #[test]
    fn render_in_a_tiny_area_does_not_panic() {
        let mut p = picker(true);
        let mut terminal = Terminal::new(TestBackend::new(3, 2)).unwrap();
        terminal.draw(|f| p.render(f, f.area())).unwrap();
    }
}
