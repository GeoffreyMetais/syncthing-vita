//! State of the `Files` tab folder browser (`FolderPicker`): what it is used for, the browsed
//! directory, its rows, the selection and the scroll window.

use crate::domain::models::{PickerItem, PickerMode, BROWSER_ROWS};
use crate::domain::scroll_window;

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
        scroll_window::scroll_by(
            &mut self.top_idx,
            BROWSER_ROWS,
            len,
            &mut self.selected_idx,
            rows,
        );
        self.clamp_scroll();
    }

    /// Keeps the selection within the list and the scroll window (`top_idx`) on the selection.
    pub fn clamp_scroll(&mut self) {
        let len = self.items.len();
        scroll_window::clamp_window(&mut self.top_idx, BROWSER_ROWS, len, &mut self.selected_idx);
    }
}
