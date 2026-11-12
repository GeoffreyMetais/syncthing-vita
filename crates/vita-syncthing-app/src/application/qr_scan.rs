//! QR code device pairing (`QrScanFlow`): turns rear-camera scanner events into a new device,
//! reusing the `AddDeviceById` confirmation (same validation, linking and toasts).

use syncthing_core::DeviceId;

use crate::application::modal_input::confirm_add_device;
use crate::application::UserCommand;
use crate::domain::{AppState, ModalDialog, UiText};
use crate::infrastructure::{ConfigRepository, QrScanEvent};

pub struct QrScanFlow;

impl QrScanFlow {
    /// Opens the QR scanner modal (the composition root starts the camera while it is open).
    pub fn open(state: &mut AppState, return_share_idx: Option<usize>) {
        state.ime_active = false;
        state.qr_preview.clear();
        state.qr_camera_error = None;
        state.modal = ModalDialog::ScanDeviceQr { return_share_idx };
    }

    /// Closes the scanner and goes back to the `AddDeviceById` modal.
    pub fn close(state: &mut AppState, return_share_idx: Option<usize>) {
        state.qr_preview.clear();
        state.modal = ModalDialog::AddDeviceById {
            return_share_idx,
            keypad_idx: 0,
        };
    }

    /// Handles one scanner event while `ModalDialog::ScanDeviceQr` is open.
    pub fn handle_event(
        state: &mut AppState,
        repo: &ConfigRepository,
        event: QrScanEvent,
    ) -> UserCommand {
        let ModalDialog::ScanDeviceQr { return_share_idx } = state.modal else {
            return UserCommand::None;
        };
        match event {
            QrScanEvent::CameraError(err) => {
                state.qr_camera_error = Some(err);
                UserCommand::None
            }
            QrScanEvent::Payload(payload) => match extract_device_id(&payload) {
                Some(device_id) => {
                    state.device_id_input = device_id;
                    let cmd = confirm_add_device(state, repo, return_share_idx);
                    if state.is_scanning_qr() {
                        // Rejected (e.g. own ID): fall back to the form showing the scanned ID
                        Self::close(state, return_share_idx);
                    } else {
                        state.qr_preview.clear();
                    }
                    cmd
                }
                None => {
                    state.show_toast(UiText::QrNotDeviceId);
                    UserCommand::None
                }
            },
        }
    }
}

/// Extracts a full Syncthing Device ID from a QR code payload: either the whole text (as shown
/// by the Syncthing GUI) or the first ID-like token of a longer text / URL.
/// Returns the canonical `XXXXXXX-XXXXXXX-...` form, or `None` for anything else (short IDs
/// are rejected: a random 7-letter word would otherwise be accepted).
pub fn extract_device_id(payload: &str) -> Option<String> {
    let whole = payload.trim();
    let tokens = whole.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'));
    std::iter::once(whole)
        .chain(tokens)
        .filter(|token| token.len() >= 52)
        .filter_map(|token| DeviceId::parse_user_id(token).ok())
        .find(|id| !id.is_short_placeholder())
        .map(|id| id.to_syncthing_string())
}
