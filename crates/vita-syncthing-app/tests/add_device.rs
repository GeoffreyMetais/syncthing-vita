//! `AddDeviceById` modal (`modal_input/add_device.rs`): Short ID entry with the system keyboard,
//! placeholder upgrade on discovery, hidden-by-default virtual keypad.

mod common;

use common::{press, temp_test_dir};
use vita_syncthing_app::application::{InputController, ShareService, UserCommand};
use vita_syncthing_app::domain::{ActiveTab, AppState, ModalDialog};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_RIGHT, BTN_SQUARE, BTN_TRIANGLE,
};

#[test]
fn test_add_device_by_short_id_and_system_keyboard_flow() {
    let dir = temp_test_dir("add-device-ime");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    let share_idx = ShareService::create_share(
        &mut state,
        &repo,
        "vita-savedata",
        "Vita Save Data",
        "ux0:user/00/savedata",
    );
    state.modal = ModalDialog::ManageDevices {
        share_idx,
        selected_dev_idx: 0,
    };

    // 1. Pressing SQUARE in ManageDevices opens AddDeviceById and requests PS Vita System Keyboard
    let cmd = press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(cmd, UserCommand::OpenSystemKeyboard);
    assert_eq!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: Some(share_idx),
            keypad_idx: 0,
        }
    );

    // 2. Simulate entering a 7-char Short ID via PS Vita System Keyboard (`SceIme`) and pressing Enter
    let peer_seed = *b"REMOTE_PC_SYNCTHING_SEED_0000001";
    let full_peer_id = syncthing_core::VitaIdentity::from_seed(&peer_seed)
        .unwrap()
        .device_id;
    let short_id = full_peer_id.short();

    let cmd2 = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            ime_text: Some(short_id.to_lowercase()),
            ime_submitted: true,
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(cmd2, UserCommand::StartSync);
    assert_eq!(state.devices.len(), 1);
    assert!(state.devices[0].device_id.is_short_placeholder());
    assert_eq!(state.devices[0].device_id.short(), short_id);
    assert!(state.devices[0].device_id.matches_peer(&full_peer_id));
    assert_eq!(
        state.shares[share_idx].shared_devices,
        vec![state.devices[0].device_id.to_hex()]
    );

    // 3. When the peer is subsequently discovered on LAN, its Short ID placeholder automatically upgrades to the full 32-byte DeviceId
    ShareService::upsert_device(
        &mut state,
        &repo,
        full_peer_id,
        "Gaming-PC",
        "192.168.1.42:22000",
    );
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].device_id, full_peer_id);
    assert!(!state.devices[0].device_id.is_short_placeholder());
    assert_eq!(
        state.shares[share_idx].shared_devices,
        vec![full_peer_id.to_hex()]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_add_device_virtual_keyboard_is_hidden_until_requested() {
    let dir = temp_test_dir("virtual-keyboard");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.active_tab = ActiveTab::Settings;

    // Triangle on Settings opens the modal (with the system keyboard) and no virtual keypad
    assert_eq!(
        press(&mut state, &repo, BTN_TRIANGLE),
        UserCommand::OpenSystemKeyboard
    );
    assert!(!state.fallback_keypad_visible);

    // While hidden, D-pad / Cross do not type on the keypad
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_CROSS);
    assert!(state.device_id_input.is_empty());
    assert_eq!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: None,
            keypad_idx: 0,
        }
    );

    // Triangle (Virtual Keyboard) shows the keypad, which then types keys
    assert_eq!(press(&mut state, &repo, BTN_TRIANGLE), UserCommand::None);
    assert!(state.fallback_keypad_visible);
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.device_id_input, "B");

    // Triangle again hides it; reopening the modal starts hidden
    press(&mut state, &repo, BTN_TRIANGLE);
    assert!(!state.fallback_keypad_visible);
    press(&mut state, &repo, BTN_TRIANGLE);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    press(&mut state, &repo, BTN_TRIANGLE);
    assert!(!state.fallback_keypad_visible);

    let _ = std::fs::remove_dir_all(&dir);
}
