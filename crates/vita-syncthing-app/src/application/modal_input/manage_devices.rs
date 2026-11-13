//! `ManageDevices` modal input: link devices to a share, pause it, switch its read-only mode,
//! revert its local changes (read only), apply & sync.

use super::open_add_device_modal;
use crate::application::{QrScanFlow, ShareCreation, ShareModeService, ShareService, UserCommand};
use crate::domain::{AppState, ModalDialog, ShareMode, UiText, VISIBLE_DEVICES};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_R, BTN_SELECT,
    BTN_SQUARE, BTN_START, BTN_TRIANGLE, BTN_UP,
};

/// Handles input in the `ManageDevices` modal; `row` is the device row touched, if any.
pub fn handle(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    row: Option<usize>,
    share_idx: usize,
    mut selected_dev_idx: usize,
) -> UserCommand {
    let dev_count = state.devices.len().min(VISIBLE_DEVICES);
    if (input.pressed & BTN_UP) != 0 && selected_dev_idx > 0 {
        selected_dev_idx -= 1;
        state.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx,
        };
    }
    if (input.pressed & BTN_DOWN) != 0 && selected_dev_idx + 1 < dev_count {
        selected_dev_idx += 1;
        state.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx,
        };
    }
    if (input.pressed & BTN_CIRCLE) != 0 {
        close_manage_devices(state, repo);
        return UserCommand::None;
    }
    if (input.pressed & BTN_SQUARE) != 0 {
        return open_add_device_modal(state, Some(share_idx));
    }
    if (input.pressed & BTN_R) != 0 {
        QrScanFlow::open(state, Some(share_idx));
        return UserCommand::None;
    }
    if (input.pressed & BTN_SELECT) != 0 {
        state.selected_share_idx = share_idx;
        ShareService::toggle_selected_share(state, repo);
        return UserCommand::None;
    }
    if (input.pressed & BTN_L) != 0 {
        ShareModeService::toggle(state, repo, share_idx);
        return UserCommand::None;
    }
    let read_only = state
        .shares
        .get(share_idx)
        .is_some_and(|sh| sh.mode == ShareMode::ReadOnly);
    if (input.pressed & BTN_START) != 0 && read_only {
        state.modal = ModalDialog::ConfirmRevertShare { share_idx };
        return UserCommand::None;
    }
    if (input.pressed & BTN_TRIANGLE) != 0 {
        return apply_and_sync(state, repo, share_idx);
    }

    let mut toggle_dev = (input.pressed & BTN_CROSS) != 0;
    if let Some(idx) = row.filter(|&idx| idx < dev_count) {
        selected_dev_idx = idx;
        state.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx,
        };
        toggle_dev = true;
    }

    if toggle_dev && selected_dev_idx < state.devices.len() {
        let dev_id = state.devices[selected_dev_idx].device_id;
        if let Some((folder_lbl, dev_name, added)) =
            ShareService::toggle_device_for_share(state, repo, share_idx, dev_id)
        {
            let action_str = if added { "Added" } else { "Removed" };
            let prep = if added { "to" } else { "from" };
            let msg = format!("{} {} {} '{}'", action_str, dev_name, prep, folder_lbl);
            state.show_toast(if added {
                UiText::DeviceLinked {
                    device: dev_name,
                    share: folder_lbl,
                }
            } else {
                UiText::DeviceUnlinked {
                    device: dev_name,
                    share: folder_lbl,
                }
            });
            ShareService::log_event(state, repo, &msg);
        }
    }
    UserCommand::None
}

/// Closes `ManageDevices`, discarding a share being created that still has no linked device.
fn close_manage_devices(state: &mut AppState, repo: &ConfigRepository) {
    state.modal = ModalDialog::None;
    ShareCreation::finish(state, repo);
}

/// `Apply & Sync` in `ManageDevices`: offers the share to its devices and (re)starts the sync.
/// With no known device yet, keeps the modal open while the LAN is scanned.
fn apply_and_sync(state: &mut AppState, repo: &ConfigRepository, share_idx: usize) -> UserCommand {
    if state.is_share_creation_incomplete(share_idx) && !state.devices.is_empty() {
        state.show_toast(UiText::LinkDeviceFirst);
        return UserCommand::None;
    }
    if let Some(share) = state.shares.get(share_idx) {
        if share.enabled && !share.shared_devices.is_empty() {
            let fid = share.folder_id.clone();
            state.queue_folder_offer(&fid);
            state.config_dirty = true;
        }
    }
    if !state.devices.is_empty() {
        close_manage_devices(state, repo);
    }
    if !state.sync_in_progress {
        return UserCommand::StartSync;
    }
    UserCommand::None
}
