//! `AddDeviceById` modal input: system keyboard, virtual keypad and device confirmation.

use crate::application::{ShareService, UserCommand};
use crate::domain::{AppState, ModalDialog, ADD_DEVICE_KEYPAD};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_SQUARE,
    BTN_START, BTN_TRIANGLE, BTN_UP,
};
use crate::ui::theme::{add_device_modal_height, VIRTUAL_KEYBOARD_BTN_W};

/// Handles input in the `AddDeviceById` modal.
pub fn handle(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
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
    if let Some((tx, ty)) = input.touch_tap {
        let w = 700usize;
        let h = add_device_modal_height(keypad) as usize;
        let dx = (960 - w) / 2;
        let dy = (500 - h) / 2;
        let lx = dx + 28;

        if ty >= 500 {
            if tx < 220 {
                state.fallback_keypad_visible = !keypad;
            } else if tx < 420 {
                return confirm_add_device(state, repo, return_share_idx);
            } else if tx >= 760 {
                close_add_device_modal(state, return_share_idx);
            }
            return UserCommand::None;
        }

        if tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h {
            // Tapping the text input box opens the PS Vita System Keyboard, except
            // its `Virtual Keyboard` button which toggles the fallback keypad
            if (dy + 76..=dy + 132).contains(&ty) {
                if tx >= dx + w - 28 - VIRTUAL_KEYBOARD_BTN_W {
                    state.fallback_keypad_visible = !keypad;
                    return UserCommand::None;
                }
                return UserCommand::OpenSystemKeyboard;
            }
            // Tapping bottom action buttons (`Add Device`, `Backspace`, `Cancel`)
            if ty >= dy + h - 52 {
                if tx >= dx + w - 135 {
                    close_add_device_modal(state, return_share_idx);
                } else if tx >= dx + w - 275 {
                    state.device_id_input.pop();
                } else if tx >= dx + w - 440 {
                    return confirm_add_device(state, repo, return_share_idx);
                }
                return UserCommand::None;
            }
            // Tapping a key in the 9x4 Base32 keypad grid
            let grid_y = dy + 162;
            let key_count = if keypad { ADD_DEVICE_KEYPAD.len() } else { 0 };
            for idx in 0..key_count {
                let col = idx % 9;
                let row = idx / 9;
                let kx = lx + col * 72;
                let ky = grid_y + row * 42;
                if tx >= kx && tx <= kx + 68 && ty >= ky && ty <= ky + 38 {
                    keypad_idx = idx;
                    state.modal = ModalDialog::AddDeviceById {
                        return_share_idx,
                        keypad_idx,
                    };
                    activate_key = true;
                    break;
                }
            }
        } else {
            close_add_device_modal(state, return_share_idx);
            return UserCommand::None;
        }
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

fn confirm_add_device(
    state: &mut AppState,
    repo: &ConfigRepository,
    return_share_idx: Option<usize>,
) -> UserCommand {
    let input_str = state.device_id_input.clone();
    match ShareService::add_device_by_input(state, repo, &input_str, return_share_idx) {
        Ok((dev_id, dev_name, is_short)) => {
            let msg = if is_short {
                format!(
                    "Added Short ID {} (will auto-pair on LAN)",
                    dev_id.syncthing_prefix()
                )
            } else {
                format!("Added device {} ({})", dev_name, dev_id.short())
            };
            ShareService::log_event(state, repo, &msg);
            state.show_toast(&msg);

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
            state.show_toast(&err);
            UserCommand::None
        }
    }
}
