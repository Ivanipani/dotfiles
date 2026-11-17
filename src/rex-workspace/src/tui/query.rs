//! A one-line text field: typing, backspace, and the usual clear/delete-word chords.
//! Chords avoid ctrl+a/h/j/k/l and alt+*, which Rex binds globally.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Query {
    pub text: String,
}

impl Query {
    #[cfg(test)]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// Applies an editing key; true when the text changed.
    pub fn edit(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Backspace => match self.text.grapheme_indices(true).next_back() {
                Some((i, _)) => {
                    self.text.truncate(i);
                    true
                }
                None => false,
            },
            KeyCode::Char('u') if ctrl => !std::mem::take(&mut self.text).is_empty(),
            KeyCode::Char('w') if ctrl => {
                let kept = self.text.trim_end().rsplit_once(' ').map_or("", |(head, _)| head).len();
                let changed = kept != self.text.len();
                self.text.truncate(kept);
                changed
            }
            KeyCode::Char(c)
                if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    && !c.is_control() =>
            {
                self.text.push(c);
                true
            }
            _ => false,
        }
    }

    /// The end of the text that fits in `width` columns.
    pub fn tail(&self, width: usize) -> &str {
        let mut used = 0;
        let mut start = self.text.len();
        for (i, g) in self.text.grapheme_indices(true).rev() {
            used += UnicodeWidthStr::width(g);
            if used > width {
                break;
            }
            start = i;
        }
        &self.text[start..]
    }
}

/// Keep untrusted labels on one line with no terminal controls.
pub fn single_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn edit_appends_a_typed_character() {
        // GIVEN
        let mut q = Query::new("ab");
        // WHEN
        let changed = q.edit(key(KeyCode::Char('c')));
        // THEN
        assert!(changed && q.text == "abc");
    }

    #[test]
    fn edit_backspace_removes_a_whole_grapheme() {
        // GIVEN a woman-technologist emoji, a ZWJ sequence
        let mut q = Query::new("a👩‍💻");
        // WHEN
        q.edit(key(KeyCode::Backspace));
        // THEN
        assert_eq!(q.text, "a");
    }

    #[test]
    fn edit_ctrl_u_clears_the_text() {
        let mut q = Query::new("abc");
        q.edit(ctrl('u'));
        assert!(q.text.is_empty());
    }

    #[test]
    fn edit_ctrl_w_deletes_the_last_word() {
        let mut q = Query::new("one two");
        q.edit(ctrl('w'));
        assert_eq!(q.text, "one");
    }

    #[test]
    fn edit_ignores_alt_chords_so_rex_keys_do_not_type() {
        let mut q = Query::default();
        let changed = q.edit(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT));
        assert!(!changed && q.text.is_empty());
    }

    #[test]
    fn tail_keeps_only_what_fits_the_width() {
        assert_eq!(Query::new("ab界").tail(2), "界");
    }

    #[test]
    fn single_line_replaces_control_characters() {
        assert_eq!(single_line("hi\x1b\nthere"), "hi  there");
    }
}
