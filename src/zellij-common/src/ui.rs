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
        return format!("{REVERSE}{glyph}{truncated}{RESET}");
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
