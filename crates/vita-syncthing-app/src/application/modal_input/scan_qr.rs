//! `ScanDeviceQr` modal input: going back to the ID form (scanner events go to `QrScanFlow`).

use crate::application::{QrScanFlow, UserCommand};
use crate::domain::AppState;
use crate::infrastructure::{InputFrame, BTN_CIRCLE};
use crate::ui::theme::{scan_qr_modal_rect, HINT_Y};

/// Handles input in the `ScanDeviceQr` modal: `Circle`, the `Back` button, a tap on
/// the hint bar or outside the modal return to `AddDeviceById`.
pub fn handle(
    state: &mut AppState,
    input: InputFrame,
    return_share_idx: Option<usize>,
) -> UserCommand {
    let mut back = (input.pressed & BTN_CIRCLE) != 0;
    if let Some((tx, ty)) = input.touch_tap {
        let (dx, dy, w, h) = scan_qr_modal_rect();
        let inside = tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h;
        let on_back_button = inside && ty >= dy + h - 52 && tx >= dx + w - 160;
        back |= ty >= HINT_Y as usize || !inside || on_back_button;
    }
    if back {
        QrScanFlow::close(state, return_share_idx);
    }
    UserCommand::None
}
