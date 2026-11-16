//! `AddDeviceById` virtual keypad and text entry (`modal_input/add_device.rs`): 9x4 cursor
//! bounds, typing with `Cross`, `Bksp` / `Clear` / `Add` keys, the 63-character limit, system
//! keyboard (IME) events and the modal's exits (`Circle`, `R`, `Triangle`, `Start`).

mod common;

use common::{draw, press, rect_of, tap_rect, temp_test_dir};
use syncthing_core::DeviceIdentity;
use vita_syncthing_app::application::{ShareService, UserCommand};
use vita_syncthing_app::domain::{AppState, ModalDialog, UiAction, UiText, ADD_DEVICE_KEYPAD};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_R, BTN_RIGHT,
    BTN_SQUARE, BTN_START, BTN_TRIANGLE, BTN_UP,
};

/// Index of `label` in the keypad.
fn key(label: &str) -> usize {
    ADD_DEVICE_KEYPAD
        .iter()
        .position(|k| *k == label)
        .unwrap_or_else(|| panic!("{label} is not a keypad key"))
}

/// State with the virtual keypad shown and the modal parked on keypad cell `keypad_idx`.
fn keypad_state(name: &str, keypad_idx: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.fallback_keypad_visible = true;
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx,
    };
    (dir, repo, state)
}

fn keypad_idx_of(state: &AppState) -> usize {
    match state.modal {
        ModalDialog::AddDeviceById { keypad_idx, .. } => keypad_idx,
        other => panic!("expected AddDeviceById, got {other:?}"),
    }
}

#[test]
fn test_left_stops_at_the_first_column_and_moves_one_cell_otherwise() {
    let (dir, repo, mut state) = keypad_state("keypad-left", 0);
    // First cell of the grid: nothing to the left.
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(keypad_idx_of(&state), 0);
    // First cell of the second row: nothing to the left either (no wrap onto the row above).
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 9,
    };
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(keypad_idx_of(&state), 9);
    // Any other cell moves exactly one cell to the left.
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 5,
    };
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(keypad_idx_of(&state), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_right_stops_at_the_last_column_and_moves_one_cell_otherwise() {
    let (dir, repo, mut state) = keypad_state("keypad-right", 8);
    // Last column of the first row: no wrap onto the next row.
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(keypad_idx_of(&state), 8);
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 7,
    };
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(keypad_idx_of(&state), 8);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_up_and_down_stay_inside_the_four_rows() {
    let (dir, repo, mut state) = keypad_state("keypad-updown", 3);
    // Up on the first row does nothing.
    press(&mut state, &repo, BTN_UP);
    assert_eq!(keypad_idx_of(&state), 3);
    // Down from the first row moves a whole row.
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(keypad_idx_of(&state), 12);
    // Up from the second row goes back one row.
    press(&mut state, &repo, BTN_UP);
    assert_eq!(keypad_idx_of(&state), 3);
    // Down on the last row does nothing (the grid has 36 keys, 4 rows).
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 27,
    };
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(keypad_idx_of(&state), 27);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_keypad_cursor_ignores_the_pad_while_the_keypad_is_hidden() {
    let (dir, repo, mut state) = keypad_state("keypad-hidden-nav", 4);
    state.fallback_keypad_visible = false;
    for button in [BTN_LEFT, BTN_RIGHT, BTN_UP, BTN_DOWN, BTN_CROSS] {
        press(&mut state, &repo, button);
    }
    assert_eq!(keypad_idx_of(&state), 4);
    assert!(state.device_id_input.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_types_the_key_under_the_cursor() {
    let (dir, repo, mut state) = keypad_state("keypad-type", key("A"));
    press(&mut state, &repo, BTN_CROSS);
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_CROSS);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.device_id_input, "ABK");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_bksp_clear_and_square_edit_the_typed_id() {
    let (dir, repo, mut state) = keypad_state("keypad-edit", key("A"));
    state.device_id_input = "ABCD".to_owned();
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: key("Bksp"),
    };
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.device_id_input, "ABC");
    // Square deletes the last character too, whatever the cursor is on.
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(state.device_id_input, "AB");
    // Pressing Bksp on an empty text is harmless.
    state.device_id_input.clear();
    press(&mut state, &repo, BTN_CROSS);
    assert!(state.device_id_input.is_empty());
    // Clear empties the text.
    state.device_id_input = "ABC".to_owned();
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: key("Clear"),
    };
    press(&mut state, &repo, BTN_CROSS);
    assert!(state.device_id_input.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_typed_id_is_capped_at_63_characters() {
    let (dir, repo, mut state) = keypad_state("keypad-cap", key("A"));
    for _ in 0..70 {
        press(&mut state, &repo, BTN_CROSS);
    }
    assert_eq!(state.device_id_input.len(), 63);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_add_key_submits_the_typed_device_id() {
    let dir = temp_test_dir("keypad-add");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let peer = DeviceIdentity::from_seed(b"KEYPAD_PEER_SEED_000000000000001")
        .unwrap()
        .device_id;
    state.device_id_input = peer.to_syncthing_string();
    state.fallback_keypad_visible = true;
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: key("Add"),
    };
    let cmd = press(&mut state, &repo, BTN_CROSS);
    assert_eq!(cmd, UserCommand::StartSync);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.devices.iter().any(|d| d.device_id == peer));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_keypad_row_touch_types_the_key_and_moves_the_cursor() {
    let (dir, repo, mut state) = keypad_state("keypad-touch", 0);
    draw(&mut state);
    let rect = rect_of(&mut state, UiAction::ModalRow(key("F")));
    tap_rect(&mut state, &repo, rect);
    assert_eq!(keypad_idx_of(&state), key("F"));
    assert_eq!(state.device_id_input, "F");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_touching_the_text_box_asks_for_the_system_keyboard() {
    let (dir, repo, mut state) = keypad_state("keypad-textbox", 0);
    let rect = rect_of(&mut state, UiAction::OpenKeyboard);
    assert_eq!(
        tap_rect(&mut state, &repo, rect),
        UserCommand::OpenSystemKeyboard
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_system_keyboard_events_set_the_text_and_submit_or_cancel() {
    let dir = temp_test_dir("keypad-ime");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 0,
    };
    state.ime_active = true;
    common::feed(
        &mut state,
        &repo,
        InputFrame {
            ime_text: Some("  abc-def ".to_owned()),
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.device_id_input, "ABC-DEF");

    common::feed(
        &mut state,
        &repo,
        InputFrame {
            ime_canceled: true,
            any_activity: true,
            ..Default::default()
        },
    );
    assert!(!state.ime_active);
    assert!(matches!(state.modal, ModalDialog::AddDeviceById { .. }));

    // An invalid ID (too short for a Short ID) is refused with a toast and keeps the modal open.
    state.device_id_input = "AB".to_owned();
    state.ime_active = true;
    let cmd = common::feed(
        &mut state,
        &repo,
        InputFrame {
            ime_submitted: true,
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(cmd, UserCommand::None);
    assert!(!state.ime_active);
    assert!(matches!(state.modal, ModalDialog::AddDeviceById { .. }));
    assert!(state.toast_msg.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_triangle_toggles_the_keypad_and_r_opens_the_scanner() {
    let dir = temp_test_dir("keypad-triangle-r");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: Some(0),
        keypad_idx: 0,
    };
    press(&mut state, &repo, BTN_TRIANGLE);
    assert!(state.fallback_keypad_visible);
    press(&mut state, &repo, BTN_TRIANGLE);
    assert!(!state.fallback_keypad_visible);
    press(&mut state, &repo, BTN_R);
    assert_eq!(
        state.modal,
        ModalDialog::ScanDeviceQr {
            return_share_idx: Some(0)
        }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_circle_returns_to_manage_devices_or_closes_the_modal() {
    let dir = temp_test_dir("keypad-circle");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let share_idx = ShareService::create_share(&mut state, &repo, "docs", "Docs", "/tmp/docs");
    state.ime_active = true;
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: Some(share_idx),
        keypad_idx: 5,
    };
    press(&mut state, &repo, BTN_CIRCLE);
    assert!(!state.ime_active);
    assert_eq!(
        state.modal,
        ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx: 0
        }
    );

    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 0,
    };
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_start_confirms_an_invalid_id_with_a_toast() {
    let dir = temp_test_dir("keypad-start");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.device_id_input = "AB".to_owned();
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 0,
    };
    let cmd = press(&mut state, &repo, BTN_START);
    assert_eq!(cmd, UserCommand::None);
    assert!(matches!(state.modal, ModalDialog::AddDeviceById { .. }));
    assert!(matches!(
        state.toast_msg,
        Some((UiText::InvalidDeviceId(_), _))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_only_up_moves_the_cursor_up_from_the_lower_rows() {
    let (dir, repo, mut state) = keypad_state("keypad-up-only", 12);
    // Cross types the key under the cursor and leaves the cursor where it is.
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(keypad_idx_of(&state), 12);
    assert_eq!(state.device_id_input, ADD_DEVICE_KEYPAD[12]);
    // Left from the lower row moves left, not up.
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(keypad_idx_of(&state), 11);
    // Right from the lower row moves right, not up.
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(keypad_idx_of(&state), 12);
    let _ = std::fs::remove_dir_all(&dir);
}
