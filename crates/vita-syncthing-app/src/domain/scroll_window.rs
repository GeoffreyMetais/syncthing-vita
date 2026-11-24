//! Scroll-window helpers shared by the `Overview` share list and the `Files` folder browser
//! (`LogView` is anchored on the newest line instead, so it keeps its own `from_end` logic):
//! a window of `visible` rows over a list of `len` items, with `top` as first visible index
//! and a selection kept inside the window.

/// First visible index after moving the window as little as needed to keep `selected` visible.
pub fn first_visible(top: usize, visible: usize, len: usize, selected: usize) -> usize {
    let max_first = len.saturating_sub(visible);
    let selected = selected.min(len.saturating_sub(1));
    let first = top.min(max_first);
    if selected < first {
        selected
    } else if selected >= first + visible {
        selected + 1 - visible
    } else {
        first
    }
}

/// Records the on-screen window (`first_visible`) as the scroll position, so that later
/// selection moves scroll from it rather than from an older position.
pub fn settle(top: &mut usize, visible: usize, len: usize, selected: usize) {
    *top = first_visible(*top, visible, len, selected);
}

/// Scrolls the window by `rows` (positive: towards the end of the list), within bounds;
/// the selection moves along so that it stays visible.
pub fn scroll_by(top: &mut usize, visible: usize, len: usize, selected: &mut usize, rows: i32) {
    if len == 0 {
        *top = 0;
        *selected = 0;
        return;
    }
    let max_first = len.saturating_sub(visible);
    let first = (first_visible(*top, visible, len, *selected) as i64 + rows as i64)
        .clamp(0, max_first as i64) as usize;
    *top = first;
    let last = (first + visible).min(len) - 1;
    *selected = (*selected).clamp(first, last);
}

/// Keeps the selection within the list and the window on the selection.
pub fn clamp_window(top: &mut usize, visible: usize, len: usize, selected: &mut usize) {
    if len == 0 {
        *top = 0;
        *selected = 0;
        return;
    }
    if *selected >= len {
        *selected = len - 1;
    }
    if *selected < *top {
        *top = *selected;
    } else if *selected >= *top + visible {
        *top = *selected + 1 - visible;
    }
}
