//! `Files` page controller (folder browser): D-pad focus over the `Up` button and the folder
//! rows, and execution of the browser's `UiAction`s. The elements are declared by
//! `ui/screens/folder_picker.rs`.

use crate::application::FolderBrowser;
use crate::domain::{AppState, NavFocus, UiAction};
use crate::infrastructure::{ConfigRepository, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP};

pub struct FilesController;

impl FilesController {
    /// Moves the D-pad focus: `Up` header button, then the folder rows. `Right` on a row
    /// opens its folder.
    pub fn move_focus(state: &mut AppState, buttons: u32) -> Option<UiAction> {
        if buttons & BTN_UP != 0 && !state.nav.header {
            if state.picker.selected_idx == 0 {
                state.nav.header = true;
            } else {
                state.picker.selected_idx -= 1;
                state.picker.clamp_scroll();
            }
        }
        if buttons & BTN_DOWN != 0 {
            if state.nav.header {
                state.nav.header = false;
            } else if state.picker.selected_idx + 1 < state.picker.items.len() {
                state.picker.selected_idx += 1;
                state.picker.clamp_scroll();
            }
        }
        if buttons & BTN_LEFT != 0 {
            state.nav.focus = NavFocus::Sidebar;
            return None;
        }
        if buttons & BTN_RIGHT != 0 && !state.nav.header {
            return Some(UiAction::PickerOpen(state.picker.selected_idx));
        }
        None
    }

    /// Scrolls the folder list by `rows` (touch drag); the selection stays visible.
    pub fn scroll(state: &mut AppState, rows: i32) {
        state.picker.scroll_rows(rows);
    }

    /// Executes a `Files` action. A row index that is no longer valid (the listing changed
    /// since the frame was drawn) is ignored.
    pub fn apply(state: &mut AppState, repo: &ConfigRepository, action: UiAction) {
        match action {
            UiAction::PickerUp => FolderBrowser::navigate_up(state, repo),
            UiAction::PickerSelect(idx) | UiAction::PickerOpen(idx) => {
                if idx >= state.picker.items.len() {
                    return;
                }
                state.picker.selected_idx = idx;
                let enter = matches!(action, UiAction::PickerOpen(_));
                FolderBrowser::apply_selection(state, repo, enter);
            }
            _ => {}
        }
    }
}
