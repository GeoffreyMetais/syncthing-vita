//! Input for the simple dialogs: `ShowDeviceId`, `ConfirmDeleteShare`, `ConfirmRevertShare`,
//! `ConfirmQuit`.
//! Touches arrive as the pad button of the element touched (see `ModalInput`).

use super::open_add_device_modal;
use crate::application::{ShareModeService, ShareService, UserCommand};
use crate::domain::{AppState, ModalDialog, UiText};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE, BTN_START, BTN_TRIANGLE,
};

/// Handles input in the `ShowDeviceId` modal.
pub fn show_device_id(state: &mut AppState, input: InputFrame) -> UserCommand {
    if (input.pressed & BTN_SQUARE) != 0 {
        return open_add_device_modal(state, None);
    }
    if (input.pressed & (BTN_CIRCLE | BTN_CROSS | BTN_TRIANGLE)) != 0 {
        state.modal = ModalDialog::None;
    }
    UserCommand::None
}

/// Handles input in the `ConfirmDeleteShare` modal.
pub fn confirm_delete_share(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    share_idx: usize,
) -> UserCommand {
    let confirm = (input.pressed & BTN_CROSS) != 0;
    let cancel = (input.pressed & (BTN_CIRCLE | BTN_SQUARE | BTN_TRIANGLE)) != 0;
    if confirm {
        state.modal = ModalDialog::None;
        if let Some(label) = ShareService::delete_share(state, repo, share_idx) {
            state.show_toast(UiText::ShareDeleted { label });
        }
    } else if cancel {
        state.modal = ModalDialog::None;
    }
    UserCommand::None
}

/// Handles input in the `ConfirmRevertShare` modal; both answers return to `ManageDevices`.
pub fn confirm_revert_share(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    share_idx: usize,
) -> UserCommand {
    let confirm = (input.pressed & BTN_CROSS) != 0;
    let cancel = (input.pressed & (BTN_CIRCLE | BTN_START | BTN_SQUARE | BTN_TRIANGLE)) != 0;
    if !confirm && !cancel {
        return UserCommand::None;
    }
    state.modal = ModalDialog::ManageDevices {
        share_idx,
        selected_dev_idx: 0,
    };
    if confirm {
        return ShareModeService::request_revert(state, repo, share_idx);
    }
    UserCommand::None
}

/// Handles input in the `ConfirmQuit` modal.
pub fn confirm_quit(state: &mut AppState, input: InputFrame) -> UserCommand {
    if (input.pressed & BTN_CROSS) != 0 {
        return UserCommand::QuitApp;
    }
    if (input.pressed & (BTN_CIRCLE | BTN_START | BTN_SQUARE | BTN_TRIANGLE)) != 0 {
        state.modal = ModalDialog::None;
        return UserCommand::None;
    }
    UserCommand::None
}
