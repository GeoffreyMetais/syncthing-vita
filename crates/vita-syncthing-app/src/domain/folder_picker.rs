//! State of the `Files` tab folder browser (`FolderPicker`): what it is used for, the browsed
//! directory, its rows, the selection and the scroll window.

use crate::domain::models::{PickerItem, PickerMode, BROWSER_ROWS};

/// The folder browser of the `Files` tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPicker {
    /// Creating a share, or choosing the target folder of an accepted folder offer.
    pub mode: PickerMode,
    pub current_dir: String,
    /// Presets and subfolders of `current_dir`.
    pub items: Vec<PickerItem>,
    pub selected_idx: usize,
    /// First visible row.
    pub top_idx: usize,
}

impl FolderPicker {
    pub fn new(browse_root: &str) -> Self {
        Self {
            mode: PickerMode::CreateNewShare,
            current_dir: browse_root.to_owned(),
            items: Vec::new(),
            selected_idx: 0,
            top_idx: 0,
        }
    }

    /// Scrolls the window by `rows` (positive: towards the end of the list), within bounds;
    /// the selection moves along so that it stays visible.
    pub fn scroll_rows(&mut self, rows: i32) {
        let len = self.items.len();
        let max_top = len.saturating_sub(BROWSER_ROWS);
        let top = (self.top_idx as i64 + rows as i64).clamp(0, max_top as i64) as usize;
        self.top_idx = top;
        let last_visible = (top + BROWSER_ROWS).min(len).saturating_sub(1);
        self.selected_idx = self.selected_idx.clamp(top, last_visible.max(top));
        self.clamp_scroll();
    }

    /// Keeps the selection within the list and the scroll window (`top_idx`) on the selection.
    pub fn clamp_scroll(&mut self) {
        if self.items.is_empty() {
            self.selected_idx = 0;
            self.top_idx = 0;
            return;
        }
        if self.selected_idx >= self.items.len() {
            self.selected_idx = self.items.len() - 1;
        }
        if self.selected_idx < self.top_idx {
            self.top_idx = self.selected_idx;
        } else if self.selected_idx >= self.top_idx + BROWSER_ROWS {
            self.top_idx = self.selected_idx + 1 - BROWSER_ROWS;
        }
    }
}
