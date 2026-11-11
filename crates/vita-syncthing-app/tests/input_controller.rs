//! `InputController`: button / touch / system keyboard input routed to `AppState` changes and
//! `UserCommand`s.

mod common;

use common::temp_test_dir;
use vita_syncthing_app::application::{InputController, ShareService, UserCommand};
use vita_syncthing_app::domain::{ActiveTab, AppState, ModalDialog, ThemeMode};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE, BTN_START,
};

fn press(state: &mut AppState, repo: &ConfigRepository, buttons: u32) -> UserCommand {
    InputController::handle_input(
        state,
        repo,
        InputFrame {
            pressed: buttons,
            any_activity: true,
            ..Default::default()
        },
    )
}

#[test]
fn test_input_controller_quit_dialog_and_theme_toggle() {
    let dir = temp_test_dir("input-ctl");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // Pressing START opens ConfirmQuit modal instead of quitting immediately
    let cmd = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            pressed: BTN_START,
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(cmd, UserCommand::None);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);

    // Pressing CROSS inside ConfirmQuit returns UserCommand::QuitApp
    let cmd2 = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            pressed: BTN_CROSS,
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(cmd2, UserCommand::QuitApp);

    // Switching to Settings and pressing CROSS toggles Dark -> OLED theme
    state.modal = ModalDialog::None;
    state.active_tab = ActiveTab::Settings;
    assert_eq!(state.theme_mode, ThemeMode::Dark);
    let _ = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            pressed: BTN_CROSS,
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.theme_mode, ThemeMode::Oled);

    let _ = std::fs::remove_dir_all(&dir);
}

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
    let cmd = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            pressed: BTN_SQUARE,
            any_activity: true,
            ..Default::default()
        },
    );
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
fn test_square_on_shares_tab_asks_confirmation_before_deleting() {
    let dir = temp_test_dir("delete-share-input");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::create_share(&mut state, &repo, "a", "Share A", &format!("{}/a", dir));
    state.active_tab = ActiveTab::Shares;

    // Square opens the confirmation; Circle cancels without deleting
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 }
    );
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.shares.len(), 1);

    // Square then Cross deletes the share
    press(&mut state, &repo, BTN_SQUARE);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.shares.is_empty());

    // Square with no share does nothing
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(state.modal, ModalDialog::None);

    let _ = std::fs::remove_dir_all(&dir);
}
