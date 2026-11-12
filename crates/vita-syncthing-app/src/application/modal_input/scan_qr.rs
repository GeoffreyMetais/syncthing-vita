//! `ScanDeviceQr` modal input: going back to the ID form (scanner events go to `QrScanFlow`).

use crate::application::{QrScanFlow, UserCommand};
use crate::domain::AppState;
use crate::infrastructure::{InputFrame, BTN_CIRCLE};

/// Handles input in the `ScanDeviceQr` modal: `Circle` (also the `Back` button, its hint and
/// a touch outside the modal) returns to `AddDeviceById`.
pub fn handle(
    state: &mut AppState,
    input: InputFrame,
    return_share_idx: Option<usize>,
) -> UserCommand {
    let back = (input.pressed & BTN_CIRCLE) != 0;
    if back {
        QrScanFlow::close(state, return_share_idx);
    }
    UserCommand::None
}
