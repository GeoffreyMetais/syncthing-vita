//! Modal dialog input (`ModalInput`): routes gamepad / touch input to the open modal's handler,
//! one file per modal (mirroring `ui/modals/`).

mod add_device;
mod create_share;
mod dialogs;
mod folder_offer;
mod manage_devices;
mod scan_qr;

pub(crate) use add_device::{confirm_add_device, open_add_device_modal};

use crate::application::UserCommand;
use crate::domain::{AppState, ModalDialog};
use crate::infrastructure::{ConfigRepository, InputFrame};

pub struct ModalInput;

impl ModalInput {
    /// Handles one frame of input while `state.modal` is open.
    pub fn handle(state: &mut AppState, repo: &ConfigRepository, input: InputFrame) -> UserCommand {
        match state.modal {
            ModalDialog::None => UserCommand::None,
            ModalDialog::CreateShare { selected_idx } => {
                create_share::handle(state, repo, input, selected_idx)
            }
            ModalDialog::ManageDevices {
                share_idx,
                selected_dev_idx,
            } => manage_devices::handle(state, repo, input, share_idx, selected_dev_idx),
            ModalDialog::AddDeviceById {
                return_share_idx,
                keypad_idx,
            } => add_device::handle(state, repo, input, return_share_idx, keypad_idx),
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
