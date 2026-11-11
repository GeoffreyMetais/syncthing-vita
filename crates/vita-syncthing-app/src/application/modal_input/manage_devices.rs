//! `ManageDevices` modal input: link devices to a share, pause it, apply & sync.

use super::open_add_device_modal;
use crate::application::{QrScanFlow, ShareCreation, ShareService, UserCommand};
use crate::domain::{AppState, ModalDialog};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_R, BTN_SELECT, BTN_SQUARE,
    BTN_TRIANGLE, BTN_UP,
};
use crate::ui::theme::{MANAGE_DEVICES_H, MANAGE_DEVICES_W};

/// Handles input in the `ManageDevices` modal.
pub fn handle(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    share_idx: usize,
    mut selected_dev_idx: usize,
) -> UserCommand {
    let dev_count = state.devices.len().min(5);
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
    if (input.pressed & BTN_TRIANGLE) != 0 {
        return apply_and_sync(state, repo, share_idx);
    }

    let mut toggle_dev = (input.pressed & BTN_CROSS) != 0;
    if let Some((tx, ty)) = input.touch_tap {
        let w = MANAGE_DEVICES_W as usize;
        let h = MANAGE_DEVICES_H as usize;
        let dx = (960 - w) / 2;
        let dy = (500 - h) / 2;
        if ty >= 500 && (450..=575).contains(&tx) {
            return open_add_device_modal(state, Some(share_idx));
        }
        if ty >= 500 && (590..=705).contains(&tx) {
            QrScanFlow::open(state, Some(share_idx));
            return UserCommand::None;
        }
        if tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h {
            // Bottom buttons inside modal (Pause / Scan QR / + Add by ID / Apply & Sync / Done)
            if ty >= dy + h - 56 {
                if tx >= dx + w - 140 {
                    close_manage_devices(state, repo);
                } else if tx >= dx + w - 330 {
                    return apply_and_sync(state, repo, share_idx);
                } else if tx >= dx + w - 445 {
                    return open_add_device_modal(state, Some(share_idx));
                } else if tx >= dx + w - 578 {
                    QrScanFlow::open(state, Some(share_idx));
                } else {
                    state.selected_share_idx = share_idx;
                    ShareService::toggle_selected_share(state, repo);
                }
                return UserCommand::None;
            }
            for i in 0..dev_count {
                let ry = dy + 92 + i * 52;
                if ty >= ry && ty <= ry + 46 {
                    selected_dev_idx = i;
                    state.modal = ModalDialog::ManageDevices {
                        share_idx,
                        selected_dev_idx,
                    };
                    toggle_dev = true;
                    break;
                }
            }
        } else {
            close_manage_devices(state, repo);
            return UserCommand::None;
        }
    }

    if toggle_dev && selected_dev_idx < state.devices.len() {
        let dev_id = state.devices[selected_dev_idx].device_id;
        if let Some((folder_lbl, dev_name, added)) =
            ShareService::toggle_device_for_share(state, repo, share_idx, dev_id)
        {
            let action_str = if added { "Added" } else { "Removed" };
            let prep = if added { "to" } else { "from" };
            let msg = format!("{} {} {} '{}'", action_str, dev_name, prep, folder_lbl);
            state.show_toast(&msg);
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
        state.show_toast("Link at least one device to create this share");
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
