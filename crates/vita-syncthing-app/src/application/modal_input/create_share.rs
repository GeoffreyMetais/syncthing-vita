//! `CreateShare` modal input: pick a share template or browse `ux0:`.

use crate::application::{ShareCreation, ShareService, UserCommand};
use crate::domain::{ActiveTab, AppState, ModalDialog, PickerMode, NEW_SHARE_TEMPLATES};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_UP,
};

/// Handles input in the `CreateShare` modal; `row` is the template touched, if any.
pub fn handle(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    row: Option<usize>,
    mut selected_idx: usize,
) -> UserCommand {
    let count = NEW_SHARE_TEMPLATES.len();
    if (input.pressed & BTN_UP) != 0 && selected_idx > 0 {
        selected_idx -= 1;
        state.modal = ModalDialog::CreateShare { selected_idx };
    }
    if (input.pressed & BTN_DOWN) != 0 && selected_idx + 1 < count {
        selected_idx += 1;
        state.modal = ModalDialog::CreateShare { selected_idx };
    }
    if (input.pressed & BTN_CIRCLE) != 0 {
        state.modal = ModalDialog::None;
        return UserCommand::None;
    }

    let mut confirm = (input.pressed & BTN_CROSS) != 0;
    if let Some(idx) = row.filter(|&idx| idx < count) {
        selected_idx = idx;
        state.modal = ModalDialog::CreateShare { selected_idx };
        confirm = true;
    }

    if confirm {
        let (fid, label, path) = NEW_SHARE_TEMPLATES[selected_idx];
        if fid == "__browse__" {
            state.modal = ModalDialog::None;
            state.picker_mode = PickerMode::CreateNewShare;
            ShareService::refresh_picker_items(state, repo);
            state.active_tab = ActiveTab::FolderPicker;
        } else {
            ShareCreation::begin(state, repo, fid, label, path);
        }
    }
    UserCommand::None
}
