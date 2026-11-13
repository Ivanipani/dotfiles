use std::collections::HashSet;

pub const RESET: &str = "\u{1b}[0m";
pub const BOLD: &str = "\u{1b}[1m";
pub const DIM: &str = "\u{1b}[2m";
pub const REVERSE: &str = "\u{1b}[7m";
pub const GREEN: &str = "\u{1b}[32m";
pub const ORANGE: &str = "\u{1b}[38;5;208m";
pub const CYAN: &str = "\u{1b}[36m";
pub const RED: &str = "\u{1b}[31m";

/// Renders one list row: a colored `glyph`, then `text` with `char_indices`
/// highlighted (fuzzy-match positions). When `selected`, the whole row is
/// shown in reverse video instead — combining that with per-character colors
/// would fight the terminal's SGR reset semantics, so selection wins outright.
pub fn render_row(glyph: &str, color: &str, text: &str, char_indices: &[usize], selected: bool, width: usize) -> String {
    let truncated = truncate(text, width.saturating_sub(glyph.chars().count()));

    if selected {
        return format!("{color}{REVERSE}{glyph}{truncated}{}{RESET}", " ".repeat(width.saturating_sub(glyph.chars().count() + truncated.chars().count())));
    }

    let highlighted = highlight(&truncated, char_indices);
    format!("{color}{glyph}{RESET}{highlighted}")
}

fn highlight(text: &str, char_indices: &[usize]) -> String {
    let indices: HashSet<usize> = char_indices.iter().copied().collect();
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if indices.contains(&i) {
            out.push_str(CYAN);
            out.push_str(BOLD);
            out.push(ch);
            out.push_str(RESET);
        } else {
            out.push(ch);
        }
    }
    out
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    let mut truncated: String = text.chars().take(keep).collect();
    truncated.push('…');
    truncated
}

/// Rows taken by the header, its rule and the footer around the body.
const CHROME_ROWS: usize = 3;

/// Rows available to the body of a screen `rows` tall.
pub fn body_height(rows: usize) -> usize {
    rows.saturating_sub(CHROME_ROWS)
}

/// A bold `title` on the left and dim `right` text flush against the right edge.
pub fn header(title: &str, right: &str, width: usize) -> String {
    let title = truncate(title, width);
    let remaining = width.saturating_sub(title.chars().count() + 1);
    let right = truncate(right, remaining);
    let gap = width.saturating_sub(title.chars().count() + right.chars().count());
    format!("{CYAN}{BOLD}{title}{RESET}{}{DIM}{right}{RESET}", " ".repeat(gap))
}

/// Footer of keybinding chips, wrapped so shortcuts remain discoverable.
pub fn key_bar(hints: &[(&str, &str)], width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for (key, desc) in hints {
        let chip = key.chars().count() + 2 + 1 + desc.chars().count();
        let gap = if out.is_empty() { 0 } else { 2 };
        let gap = if used > 0 && used + gap + chip > width {
            out.push('\n');
            used = 0;
            0
        } else { gap };
        out.push_str(&" ".repeat(gap));
        let key = truncate(key, width.saturating_sub(2));
        let desc = truncate(desc, width.saturating_sub(key.chars().count() + 3));
        out.push_str(&format!("{CYAN}{REVERSE} {key} {RESET} {desc}"));
        used += gap + key.chars().count() + 3 + desc.chars().count();
    }
    out
}

/// A labelled text input with a cursor; labels are padded to `label_width` so stacked inputs align.
pub fn input(label: &str, label_width: usize, text: &str) -> String {
    format!("{BOLD}{label:<label_width$}{RESET} {text}█")
}

/// Draws header, rule, `body` (padded to fill the screen) and a footer pinned to the last row.
pub fn draw(header: &str, body: &[String], footer: &str, rows: usize, cols: usize) {
    let height = body_height(rows).saturating_sub(footer.lines().count().saturating_sub(1));
    println!("{header}");
    println!("{DIM}{}{RESET}", "─".repeat(cols));
    for line in body.iter().take(height) {
        println!("{line}");
    }
    for _ in body.len().min(height)..height {
        println!();
    }
    print!("{footer}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_wraps_without_losing_actions() {
        let bar = key_bar(&[("enter", "open"), ("esc", "back"), ("^r", "refresh")], 16);
        assert_eq!(bar.lines().count(), 3);
        for action in ["open", "back", "refresh"] {
            assert!(bar.contains(action));
        }
    }
}
