//! Modal dialog input (`ModalInput`): routes gamepad / touch input to the open modal's handler,
//! one file per modal (mirroring `ui/modals/`).
//!
//! Handlers only know buttons and rows. A touch is resolved here against the elements the modal
//! declared while drawing (`HitMap`): touching a button is the same as pressing the pad button
//! it shows, touching a list row or keypad key activates it.

mod add_device;
mod create_share;
mod dialogs;
mod folder_offer;
mod manage_devices;
mod scan_qr;

pub(crate) use add_device::{confirm_add_device, open_add_device_modal};

use crate::application::UserCommand;
use crate::domain::{AppState, ModalDialog, PadKey, UiAction};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_L, BTN_R, BTN_SELECT, BTN_SQUARE,
    BTN_START, BTN_TRIANGLE,
};

/// What a touch on the open modal activated, besides buttons (merged into the pressed mask).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalTouch {
    /// Nothing, or a button (see `InputFrame::pressed`).
    None,
    /// A list row or keypad key.
    Row(usize),
    /// The text field: open the system keyboard.
    Keyboard,
}

/// Pad button mask of `key`.
fn button_mask(key: PadKey) -> u32 {
    match key {
        PadKey::Cross => BTN_CROSS,
        PadKey::Circle => BTN_CIRCLE,
        PadKey::Square => BTN_SQUARE,
        PadKey::Triangle => BTN_TRIANGLE,
        PadKey::L => BTN_L,
        PadKey::R => BTN_R,
        PadKey::Start => BTN_START,
        PadKey::Select => BTN_SELECT,
    }
}

pub struct ModalInput;

impl ModalInput {
    /// Handles one frame of input while `state.modal` is open.
    pub fn handle(
        state: &mut AppState,
        repo: &ConfigRepository,
        mut input: InputFrame,
    ) -> UserCommand {
        // Resolve the touch against the modal's declared elements; handlers never see coordinates.
        let mut touch = ModalTouch::None;
        if let Some((tx, ty)) = input.touch_tap.take() {
            match state.hit_map.action_at(tx as i32, ty as i32) {
                Some(UiAction::Press(key)) => input.pressed |= button_mask(key),
                Some(UiAction::ModalRow(idx)) => touch = ModalTouch::Row(idx),
                Some(UiAction::OpenKeyboard) => touch = ModalTouch::Keyboard,
                _ => {}
            }
        }
        let row = match touch {
            ModalTouch::Row(idx) => Some(idx),
            _ => None,
        };

        match state.modal {
            ModalDialog::None => UserCommand::None,
            ModalDialog::CreateShare { selected_idx } => {
                create_share::handle(state, repo, input, row, selected_idx)
            }
            ModalDialog::ManageDevices {
                share_idx,
                selected_dev_idx,
            } => manage_devices::handle(state, repo, input, row, share_idx, selected_dev_idx),
            ModalDialog::AddDeviceById {
                return_share_idx,
                keypad_idx,
            } => add_device::handle(state, repo, input, touch, return_share_idx, keypad_idx),
            ModalDialog::ScanDeviceQr { return_share_idx } => {
                scan_qr::handle(state, input, return_share_idx)
            }
            ModalDialog::ShowDeviceId => dialogs::show_device_id(state, input),
            ModalDialog::ConfirmDeleteShare { share_idx } => {
                dialogs::confirm_delete_share(state, repo, input, share_idx)
            }
            ModalDialog::ConfirmQuit => dialogs::confirm_quit(state, input),
            ModalDialog::FolderOffer { offer_idx } => {
                folder_offer::handle(state, repo, input, offer_idx)
            }
        }
    }
}
