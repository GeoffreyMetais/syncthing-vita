//! Share list controller (`Overview` page): execution of the share `UiAction`s. The elements
//! themselves (position, action, focus slot, shortcut) are declared by `ui/screens/shares.rs`
//! in the frame's `HitMap`; D-pad focus over them is moved by `DashboardController`.

use crate::application::ShareService;
use crate::domain::{ActiveTab, AppState, ModalDialog, PickerMode, UiAction};
use crate::infrastructure::ConfigRepository;

pub struct SharesController;

impl SharesController {
    /// Executes a share action. A share index that is no longer valid (the list changed
    /// since the frame was drawn) is ignored.
    pub fn apply(state: &mut AppState, repo: &ConfigRepository, action: UiAction) {
        let share_idx = match action {
            UiAction::NewShare => {
                state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                return;
            }
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
            UiAction::ChangeShareFolder(_) => {
                state.picker_mode = PickerMode::ChangeShareTarget;
                ShareService::refresh_picker_items(state, repo);
                state.active_tab = ActiveTab::FolderPicker;
            }
            _ => {}
        }
    }
}
