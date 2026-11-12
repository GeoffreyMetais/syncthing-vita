//! `Shares` page controller: D-pad focus over the page's elements and execution of their
//! `UiAction`s. The elements themselves (position, action, focus slot, shortcut) are declared
//! by `ui/screens/shares.rs` in the frame's `HitMap`.

use crate::application::{DpadNavigation, ShareService};
use crate::domain::{action_count, ActiveTab, AppState, ModalDialog, PickerMode, UiAction};
use crate::infrastructure::{ConfigRepository, BTN_DOWN, BTN_UP};

pub struct SharesController;

impl SharesController {
    /// Moves the D-pad focus: `+ New Share` header, then one row per share with its buttons.
    pub fn move_focus(state: &mut AppState, buttons: u32) -> Option<UiAction> {
        let count = state.shares.len();
        if count == 0 {
            state.nav.header = true;
        }
        if buttons & BTN_UP != 0 && !state.nav.header {
            if state.selected_share_idx == 0 {
                state.nav.header = true;
            } else {
                state.selected_share_idx -= 1;
            }
        }
        if buttons & BTN_DOWN != 0 {
            if state.nav.header {
                state.nav.header = count == 0;
            } else if state.selected_share_idx + 1 < count {
                state.selected_share_idx += 1;
            }
        }
        DpadNavigation::move_in_row(state, action_count(ActiveTab::Shares), buttons);
        None
    }

    /// Executes a `Shares` action. A share index that is no longer valid (the list changed
    /// since the frame was drawn) is ignored.
    pub fn apply(state: &mut AppState, repo: &ConfigRepository, action: UiAction) {
        let share_idx = match action {
            UiAction::NewShare => {
                state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                return;
            }
            UiAction::ToggleSelectedShare => {
                ShareService::toggle_selected_share(state, repo);
                return;
            }
            UiAction::ChangeSelectedShareFolder => state.selected_share_idx,
            UiAction::SelectShare(idx)
            | UiAction::ManageDevices(idx)
            | UiAction::DeleteShare(idx)
            | UiAction::ChangeShareFolder(idx) => idx,
            _ => return,
        };
        if share_idx >= state.shares.len() {
            return;
        }
        state.selected_share_idx = share_idx;
        match action {
            UiAction::ManageDevices(_) => {
                state.modal = ModalDialog::ManageDevices {
                    share_idx,
                    selected_dev_idx: 0,
                };
            }
            UiAction::DeleteShare(_) => {
                state.modal = ModalDialog::ConfirmDeleteShare { share_idx };
            }
            UiAction::ChangeShareFolder(_) | UiAction::ChangeSelectedShareFolder => {
                state.picker_mode = PickerMode::ChangeShareTarget;
                ShareService::refresh_picker_items(state, repo);
                state.active_tab = ActiveTab::FolderPicker;
            }
            _ => {}
        }
    }
}
