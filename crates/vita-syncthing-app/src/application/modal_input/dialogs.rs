//! Input for the simple dialogs: `ShowDeviceId`, `ConfirmDeleteShare`, `ConfirmQuit`.

use super::open_add_device_modal;
use crate::application::{ShareService, UserCommand};
use crate::domain::{AppState, ModalDialog};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE, BTN_START, BTN_TRIANGLE,
};

/// Handles input in the `ShowDeviceId` modal.
pub fn show_device_id(state: &mut AppState, input: InputFrame) -> UserCommand {
    if (input.pressed & BTN_SQUARE) != 0 {
        return open_add_device_modal(state, None);
    }
    if let Some((tx, ty)) = input.touch_tap {
        let w = 720usize;
        let h = 414usize;
        let dx = (960 - w) / 2;
        let dy = (500 - h) / 2;
        if (ty >= dy + h - 56 && ty <= dy + h - 10 && tx >= dx + w - 350 && tx <= dx + w - 130)
            || (ty >= 500 && tx < 320)
        {
            return open_add_device_modal(state, None);
        }
        state.modal = ModalDialog::None;
        return UserCommand::None;
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
    let mut confirm = (input.pressed & BTN_CROSS) != 0;
    let mut cancel = (input.pressed & (BTN_CIRCLE | BTN_SQUARE | BTN_TRIANGLE)) != 0;
    if let Some((tx, ty)) = input.touch_tap {
        let w = 520usize;
        let h = 236usize;
        let dx = (960 - w) / 2;
        let dy = (500 - h) / 2;
        let on_delete_btn =
            tx >= dx + w - 160 && tx <= dx + w - 16 && ty >= dy + h - 56 && ty <= dy + h - 12;
        if on_delete_btn || (ty >= 500 && tx < 200) {
            confirm = true;
        } else {
            cancel = true;
        }
    }
    if confirm {
        state.modal = ModalDialog::None;
        if let Some(label) = ShareService::delete_share(state, repo, share_idx) {
            state.show_toast(&format!("Deleted share '{}'", label));
        }
    } else if cancel {
        state.modal = ModalDialog::None;
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
    if let Some((tx, ty)) = input.touch_tap {
        let w = 520usize;
        let h = 236usize;
        let dx = (960 - w) / 2;
        let dy = (500 - h) / 2;
        // Check if user tapped "Quit App" button at bottom-right of modal
        if tx >= dx + w - 160 && tx <= dx + w - 16 && ty >= dy + h - 56 && ty <= dy + h - 12 {
            return UserCommand::QuitApp;
        }
        // Check if user tapped "Confirm Quit" in bottom hint bar
        if ty >= 500 && tx < 200 {
            return UserCommand::QuitApp;
        }
        state.modal = ModalDialog::None;
    }
    UserCommand::None
}
