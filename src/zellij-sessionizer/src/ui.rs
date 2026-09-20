use zellij_common::ui::{render_row as render_glyph_row, DIM, GREEN, ORANGE};

/// Status of a candidate directory relative to the plugin's own session list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RowStatus {
    Current,
    Active,
    Inactive,
}

pub fn bullet(status: RowStatus) -> (&'static str, &'static str) {
    match status {
        RowStatus::Current => ("◉ ", GREEN),
        RowStatus::Active => ("● ", ORANGE),
        RowStatus::Inactive => ("  ", DIM),
    }
}

pub fn render_row(text: &str, char_indices: &[usize], status: RowStatus, selected: bool, width: usize) -> String {
    let (glyph, color) = bullet(status);
    render_glyph_row(glyph, color, text, char_indices, selected, width)
}
