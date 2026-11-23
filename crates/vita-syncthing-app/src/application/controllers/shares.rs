//! Share list controller (`Overview` page): execution of the share `UiAction`s. The elements
//! themselves (position, action, focus slot, shortcut) are declared by `ui/screens/shares.rs`
//! in the frame's `HitMap`; D-pad focus over them is moved by `DashboardController`.

use crate::domain::{AppState, ModalDialog, UiAction};

pub struct SharesController;

impl SharesController {
    /// Scrolls the share list by `rows` (touch drag); the selection stays visible.
    pub fn scroll(state: &mut AppState, rows: i32) {
        state.scroll_shares(rows);
    }

    /// Executes a share action. A share index that is no longer valid (the list changed
    /// since the frame was drawn) is ignored.
    pub fn apply(state: &mut AppState, action: UiAction) {
        let share_idx = match action {
            UiAction::NewShare => {
                state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                return;
            }
            UiAction::SelectShare(idx)
            | UiAction::ManageDevices(idx)
            | UiAction::DeleteShare(idx) => idx,
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
            _ => {}
        }
    }
}
