//! LAN scan of the `ManageDevices` dialog, so nearby Syncthing peers appear without pressing
//! `Triangle`, without useless scans:
//! - a share with no linked device scans as soon as the dialog opens;
//! - a share that already has a linked device scans only if the dialog stays open for
//!   `DEVICES_DIALOG_SCAN_DELAY` (cancelled when it closes).
//!
//! The scan also runs while another device is connected: the running worker is asked for a
//! discovery cycle right away instead of waiting for its periodic one (`UserCommand::ScanLan`).

use super::UserCommand;
use crate::domain::defaults::DEVICES_DIALOG_SCAN_DELAY;
use crate::domain::{AppState, ModalDialog};

/// Plans and starts the `ManageDevices` LAN scan.
pub struct DevicesScan;

impl DevicesScan {
    /// Called after each input frame with the dialog open before it: when `ManageDevices` has
    /// just opened, scans now (no linked device) or arms the delayed scan.
    pub fn on_input(before: ModalDialog, state: &mut AppState) -> UserCommand {
        let was_open = matches!(before, ModalDialog::ManageDevices { .. });
        let ModalDialog::ManageDevices { share_idx, .. } = state.modal else {
            state.devices_scan_countdown = None;
            return UserCommand::None;
        };
        if was_open {
            return UserCommand::None;
        }
        let has_linked_device = state
            .shares
            .get(share_idx)
            .is_some_and(|share| !share.shared_devices.is_empty());
        if has_linked_device {
            state.devices_scan_countdown = Some(DEVICES_DIALOG_SCAN_DELAY);
            UserCommand::None
        } else {
            state.devices_scan_countdown = None;
            UserCommand::ScanLan
        }
    }

    /// Advances the delayed scan by one display VBlank; cancels it once the dialog is closed.
    pub fn tick(state: &mut AppState) -> UserCommand {
        if !matches!(state.modal, ModalDialog::ManageDevices { .. }) {
            state.devices_scan_countdown = None;
            return UserCommand::None;
        }
        match state.devices_scan_countdown {
            Some(left) if left > 1 => {
                state.devices_scan_countdown = Some(left - 1);
                UserCommand::None
            }
            Some(_) => {
                state.devices_scan_countdown = None;
                UserCommand::ScanLan
            }
            None => UserCommand::None,
        }
    }
}
