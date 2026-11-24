//! Scroll window over a list of rows: the first visible index `top`, clamped so the window
//! stays within the list, with a selection kept inside the window. Shared by the `Overview`
//! share list and the `Files` folder browser (`LogView` is anchored on the newest line
//! instead, so it keeps its own `from_end` logic).

/// A scroll window showing `visible` rows of a list of `len` items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollWindow {
    /// First visible index.
    pub top: usize,
    /// Number of rows visible at once.
    pub visible: usize,
}

impl ScrollWindow {
    pub const fn new(visible: usize) -> Self {
        Self { top: 0, visible }
    }

    /// First visible index after moving the window as little as needed to keep `selected`
    /// visible.
    pub fn first_visible(&self, len: usize, selected: usize) -> usize {
        let max_first = len.saturating_sub(self.visible);
        let selected = selected.min(len.saturating_sub(1));
        let first = self.top.min(max_first);
        if selected < first {
            selected
        } else if selected >= first + self.visible {
            selected + 1 - self.visible
        } else {
            first
        }
    }

    /// Records the on-screen window (`first_visible`) as the scroll position, so that later
    /// selection moves scroll from it rather than from an older position.
    pub fn settle(&mut self, len: usize, selected: usize) {
        self.top = self.first_visible(len, selected);
    }

    /// Scrolls the window by `rows` (positive: towards the end of the list), within bounds;
    /// the selection moves along so that it stays visible.
    pub fn scroll_by(&mut self, len: usize, selected: &mut usize, rows: i32) {
        if len == 0 {
            self.top = 0;
            *selected = 0;
            return;
        }
        let max_first = len.saturating_sub(self.visible);
        let first = (self.first_visible(len, *selected) as i64 + rows as i64)
            .clamp(0, max_first as i64) as usize;
        self.top = first;
        let last = (first + self.visible).min(len) - 1;
        *selected = (*selected).clamp(first, last);
    }

    /// Keeps the selection within the list and the window on the selection.
    pub fn clamp(&mut self, len: usize, selected: &mut usize) {
        if len == 0 {
            self.top = 0;
            *selected = 0;
            return;
        }
        if *selected >= len {
            *selected = len - 1;
        }
        if *selected < self.top {
            self.top = *selected;
        } else if *selected >= self.top + self.visible {
            self.top = *selected + 1 - self.visible;
        }
    }
}
