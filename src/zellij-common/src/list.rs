/// Moves `cursor` by `delta` within `len` rows; returns whether it changed.
pub fn move_cursor(cursor: &mut usize, len: usize, delta: isize) -> bool {
    if len == 0 {
        return false;
    }
    let next = (*cursor as isize + delta).clamp(0, len as isize - 1) as usize;
    if next == *cursor {
        return false;
    }
    *cursor = next;
    true
}

/// The slice of rows to draw so `cursor` stays visible in `height` rows.
pub fn scroll_window(cursor: usize, len: usize, height: usize) -> std::ops::Range<usize> {
    if height == 0 || len == 0 {
        return 0..0;
    }
    let start = cursor.saturating_sub(height.saturating_sub(1)).min(len.saturating_sub(height.min(len)));
    start..len.min(start + height)
}
