//! `Files` page controller (folder browser): D-pad focus over the `Up` button and the folder
//! rows, and execution of the browser's `UiAction`s. The elements are declared by
//! `ui/screens/folder_picker.rs`.

use crate::application::ShareService;
use crate::domain::{AppState, NavFocus, UiAction};
use crate::infrastructure::{ConfigRepository, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP};

pub struct FilesController;

impl FilesController {
    /// Moves the D-pad focus: `Up` header button, then the folder rows. `Right` on a row
    /// opens its folder.
    pub fn move_focus(state: &mut AppState, buttons: u32) -> Option<UiAction> {
        if buttons & BTN_UP != 0 && !state.nav.header {
            if state.selected_picker_idx == 0 {
                state.nav.header = true;
            } else {
                state.selected_picker_idx -= 1;
                state.clamp_picker_scroll();
            }
        }
        if buttons & BTN_DOWN != 0 {
            if state.nav.header {
                state.nav.header = false;
            } else if state.selected_picker_idx + 1 < state.picker_items.len() {
                state.selected_picker_idx += 1;
                state.clamp_picker_scroll();
            }
        }
        if buttons & BTN_LEFT != 0 {
            state.nav.focus = NavFocus::Sidebar;
            return None;
        }
        if buttons & BTN_RIGHT != 0 && !state.nav.header {
            return Some(UiAction::PickerOpen(state.selected_picker_idx));
        }
        None
    }

    /// Executes a `Files` action. A row index that is no longer valid (the listing changed
    /// since the frame was drawn) is ignored.
    pub fn apply(state: &mut AppState, repo: &ConfigRepository, action: UiAction) {
        match action {
            UiAction::PickerUp => ShareService::navigate_picker_up(state, repo),
            UiAction::PickerSelect(idx) | UiAction::PickerOpen(idx) => {
                if idx >= state.picker_items.len() {
                    return;
                }
                state.selected_picker_idx = idx;
                let enter = matches!(action, UiAction::PickerOpen(_));
                ShareService::apply_picker_selection(state, repo, enter);
            }
            _ => {}
        }
    }
}
