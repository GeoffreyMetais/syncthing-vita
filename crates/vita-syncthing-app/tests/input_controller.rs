//! `InputController`: button / touch / system keyboard input routed to `AppState` changes and
//! `UserCommand`s.

mod common;

use common::temp_test_dir;
use vita_syncthing_app::application::{InputController, ShareService, UserCommand};
use vita_syncthing_app::domain::{ActiveTab, AppState, ModalDialog, NavFocus, ThemeMode};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_RIGHT, BTN_SELECT, BTN_SQUARE,
    BTN_START, BTN_TRIANGLE,
};
use vita_syncthing_app::ui::theme::{CONFIRM_QUIT_H, CONFIRM_QUIT_W};

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

    // Switching to Settings and pressing CROSS toggles OLED -> Dark theme and persists it
    state.modal = ModalDialog::None;
    state.active_tab = ActiveTab::Settings;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    assert_eq!(state.theme_mode, ThemeMode::Oled);
    let _ = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            pressed: BTN_CROSS,
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.theme_mode, ThemeMode::Dark);
    assert_eq!(repo.load_theme(), ThemeMode::Dark);

    // Starting a new session preserves the persisted theme
    let mut restarted = AppState::new(Vec::new(), Vec::new(), &dir);
    restarted.theme_mode = repo.load_theme();
    assert_eq!(restarted.theme_mode, ThemeMode::Dark);

    // Toggling again switches back to OLED and persists
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
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

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

#[test]
fn test_files_tab_up_navigation_walks_device_root_paths() {
    let dir = temp_test_dir("picker-up");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:data/retroarch");
    state.active_tab = ActiveTab::FolderPicker;
    state.selected_picker_idx = 3;

    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.picker_current_dir, "ux0:data");
    assert_eq!(state.selected_picker_idx, 0);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.picker_current_dir, "ux0:");
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.active_tab, ActiveTab::Shares);

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

#[test]
fn test_theme_persistence_roundtrip_and_touch() {
    let dir = temp_test_dir("theme-persistence");
    let repo = ConfigRepository::new(&dir);

    // Missing file defaults to OLED
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

    // Corrupted file content falls back to OLED
    std::fs::write(format!("{}/theme.cfg", dir), "invalid_theme_value\n").unwrap();
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.theme_mode = repo.load_theme();
    assert_eq!(state.theme_mode, ThemeMode::Oled);

    state.active_tab = ActiveTab::Settings;

    // 1. Touch on Settings display card toggles to Dark and persists
    let _ = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_tap: Some((300, 120)), // Inside Display & Theme Card (tx >= 236, 86..=204)
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.theme_mode, ThemeMode::Dark);
    assert_eq!(repo.load_theme(), ThemeMode::Dark);

    // 2. Square button on Settings tab toggles back to OLED and persists
    let _ = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            pressed: BTN_SQUARE,
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.theme_mode, ThemeMode::Oled);
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

    // 3. Touch on bottom bar (tx < 230, ty >= 490) toggles to Dark and persists
    let _ = InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_tap: Some((100, 500)),
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.theme_mode, ThemeMode::Dark);
    assert_eq!(repo.load_theme(), ThemeMode::Dark);

    // 4. Starting a new session preserves the persisted theme
    let mut new_session = AppState::new(Vec::new(), Vec::new(), &dir);
    new_session.theme_mode = repo.load_theme();
    assert_eq!(new_session.theme_mode, ThemeMode::Dark);

    let _ = std::fs::remove_dir_all(&dir);
}

fn tap(state: &mut AppState, repo: &ConfigRepository, x: usize, y: usize) -> UserCommand {
    InputController::handle_input(
        state,
        repo,
        InputFrame {
            touch_tap: Some((x, y)),
            any_activity: true,
            ..Default::default()
        },
    )
}

#[test]
fn test_confirm_quit_touch_targets_follow_dialog_size() {
    let dir = temp_test_dir("input-ctl-quit-touch");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // The `Quit App` action sits at the bottom right of the (720x236) dialog.
    let dx = (960 - CONFIRM_QUIT_W as usize) / 2;
    let dy = (500 - CONFIRM_QUIT_H as usize) / 2;
    let quit_x = dx + CONFIRM_QUIT_W as usize - 80;
    let quit_y = dy + CONFIRM_QUIT_H as usize - 34;

    state.modal = ModalDialog::ConfirmQuit;
    assert_eq!(tap(&mut state, &repo, quit_x, quit_y), UserCommand::QuitApp);

    // Tapping the dialog body cancels instead of quitting.
    state.modal = ModalDialog::ConfirmQuit;
    assert_eq!(tap(&mut state, &repo, 480, dy + 60), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::None);
}

#[test]
fn test_screensaver_wakes_on_press_or_touch_only() {
    let dir = temp_test_dir("saver-wake");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");

    press(&mut state, &repo, BTN_SELECT);
    assert!(state.oled_saver_active);
    // Activity without a press or touch contact (scripted `wait` frames) keeps it running.
    let idle = InputFrame {
        any_activity: true,
        ..Default::default()
    };
    InputController::handle_input(&mut state, &repo, idle);
    assert!(state.oled_saver_active);
    // Resting a finger on the screen wakes it.
    let touch = InputFrame {
        touch_pos: Some((100, 100)),
        any_activity: true,
        ..Default::default()
    };
    InputController::handle_input(&mut state, &repo, touch);
    assert!(!state.oled_saver_active);
    // So does any button.
    press(&mut state, &repo, BTN_SELECT);
    press(&mut state, &repo, BTN_CIRCLE);
    assert!(!state.oled_saver_active);
}
