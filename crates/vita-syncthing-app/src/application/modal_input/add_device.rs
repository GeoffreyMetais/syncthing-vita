//! `AddDeviceById` modal input: system keyboard, virtual keypad, QR scanner entry and device
//! confirmation.

use super::ModalTouch;
use crate::application::{QrScanFlow, ShareService, UserCommand};
use crate::domain::{AppState, ModalDialog, UiText, ADD_DEVICE_KEYPAD};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_R, BTN_RIGHT,
    BTN_SQUARE, BTN_START, BTN_TRIANGLE, BTN_UP,
};

/// Handles input in the `AddDeviceById` modal.
pub fn handle(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    touch: ModalTouch,
    return_share_idx: Option<usize>,
    mut keypad_idx: usize,
) -> UserCommand {
    // 1. Live events from the native PS Vita System Keyboard (`SceIme`)
    if let Some(ime_str) = input.ime_text {
        state.device_id_input = ime_str.trim().to_uppercase();
    }
    if input.ime_submitted {
        state.ime_active = false;
        return confirm_add_device(state, repo, return_share_idx);
    }
    if input.ime_canceled {
        state.ime_active = false;
        return UserCommand::None;
    }

    // 2. Gamepad actions when the OS keyboard overlay is closed
    if (input.pressed & BTN_TRIANGLE) != 0 {
        state.fallback_keypad_visible = !state.fallback_keypad_visible;
        return UserCommand::None;
    }
    if (input.pressed & BTN_R) != 0 {
        QrScanFlow::open(state, return_share_idx);
        return UserCommand::None;
    }
    if (input.pressed & BTN_START) != 0 {
        return confirm_add_device(state, repo, return_share_idx);
    }
    if (input.pressed & BTN_CIRCLE) != 0 {
        close_add_device_modal(state, return_share_idx);
        return UserCommand::None;
    }
    if (input.pressed & BTN_SQUARE) != 0 {
        state.device_id_input.pop();
    }

    // 9x4 Keypad navigation (only while the virtual keyboard is shown)
    let keypad = state.fallback_keypad_visible;
    let pressed = if keypad { input.pressed } else { 0 };
    if (pressed & BTN_LEFT) != 0 && keypad_idx % 9 > 0 {
        keypad_idx -= 1;
        state.modal = ModalDialog::AddDeviceById {
            return_share_idx,
            keypad_idx,
        };
    }
    if (pressed & BTN_RIGHT) != 0 && keypad_idx % 9 < 8 {
        keypad_idx += 1;
        state.modal = ModalDialog::AddDeviceById {
            return_share_idx,
            keypad_idx,
        };
    }
    if (pressed & BTN_UP) != 0 && keypad_idx >= 9 {
        keypad_idx -= 9;
        state.modal = ModalDialog::AddDeviceById {
            return_share_idx,
            keypad_idx,
        };
    }
    if (pressed & BTN_DOWN) != 0 && keypad_idx + 9 < ADD_DEVICE_KEYPAD.len() {
        keypad_idx += 9;
        state.modal = ModalDialog::AddDeviceById {
            return_share_idx,
            keypad_idx,
        };
    }

    let mut activate_key = (pressed & BTN_CROSS) != 0;
    match touch {
        ModalTouch::Keyboard => return UserCommand::OpenSystemKeyboard,
        ModalTouch::Row(idx) if keypad && idx < ADD_DEVICE_KEYPAD.len() => {
            keypad_idx = idx;
            state.modal = ModalDialog::AddDeviceById {
                return_share_idx,
                keypad_idx,
            };
            activate_key = true;
        }
        ModalTouch::Row(_) | ModalTouch::None => {}
    }

    if activate_key {
        let key_label = ADD_DEVICE_KEYPAD[keypad_idx];
        match key_label {
            "Bksp" => {
                state.device_id_input.pop();
            }
            "Clear" => {
                state.device_id_input.clear();
            }
            "Add" => {
                return confirm_add_device(state, repo, return_share_idx);
            }
            ch => {
                if state.device_id_input.len() < 63 {
                    state.device_id_input.push_str(ch);
                }
            }
        }
    }

    UserCommand::None
}

pub(crate) fn open_add_device_modal(
    state: &mut AppState,
    return_share_idx: Option<usize>,
) -> UserCommand {
    state.device_id_input.clear();
    state.fallback_keypad_visible = false;
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx,
        keypad_idx: 0,
    };
    UserCommand::OpenSystemKeyboard
}

fn close_add_device_modal(state: &mut AppState, return_share_idx: Option<usize>) {
    state.ime_active = false;
    if let Some(share_idx) = return_share_idx {
        state.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx: 0,
        };
    } else {
        state.modal = ModalDialog::None;
    }
}

pub(crate) fn confirm_add_device(
    state: &mut AppState,
    repo: &ConfigRepository,
    return_share_idx: Option<usize>,
) -> UserCommand {
    let input_str = state.device_id_input.clone();
    match ShareService::add_device_by_input(state, repo, &input_str, return_share_idx) {
        Ok((dev_id, dev_name, is_short)) => {
            let (msg, toast) = if is_short {
                (
                    format!(
                        "Added Short ID {} (will auto-pair on LAN)",
                        dev_id.syncthing_prefix()
                    ),
                    UiText::AddedShortId {
                        prefix: dev_id.syncthing_prefix(),
                    },
                )
            } else {
                (
                    format!("Added device {} ({})", dev_name, dev_id.short()),
                    UiText::AddedDevice {
                        name: dev_name.clone(),
                        short: dev_id.short(),
                    },
                )
            };
            ShareService::log_event(state, repo, &msg);
            state.show_toast(toast);

            if let Some(share_idx) = return_share_idx {
                let selected_dev_idx = state
                    .devices
                    .iter()
                    .position(|d| d.device_id == dev_id)
                    .unwrap_or(0)
                    .min(4);
                state.modal = ModalDialog::ManageDevices {
                    share_idx,
                    selected_dev_idx,
                };
            } else {
                state.modal = ModalDialog::None;
            }

            if !state.sync_in_progress {
                UserCommand::StartSync
            } else {
                UserCommand::None
            }
        }
        Err(err) => {
            state.show_toast(err);
            UserCommand::None
        }
    }
}
