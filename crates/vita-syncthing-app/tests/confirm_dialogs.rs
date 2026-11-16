//! Confirmation and information dialogs (`modal_input/dialogs.rs`): `ConfirmDeleteShare`,
//! `ConfirmRevertShare` (back to `ManageDevices`, revert queued for read-only shares only),
//! `ConfirmQuit` and `ShowDeviceId` transitions.

mod common;

use common::{press, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{
    DeviceRegistry, ShareModeService, ShareService, UserCommand,
};
use vita_syncthing_app::domain::{AppState, ModalDialog, ShareMode, UiText};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_R, BTN_SQUARE, BTN_START, BTN_TRIANGLE,
};

const PEER: DeviceId = DeviceId([0x61; 32]);

/// One share `Saves` (index 0) linked to `PEER`.
fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    DeviceRegistry::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER);
    (dir, repo, state)
}

#[test]
fn test_delete_share_cancel_buttons_keep_the_share() {
    for cancel in [BTN_CIRCLE, BTN_SQUARE, BTN_TRIANGLE] {
        let (dir, repo, mut state) = setup("confirm-delete-cancel");
        state.modal = ModalDialog::ConfirmDeleteShare { share_idx: 0 };
        press(&mut state, &repo, cancel);
        assert_eq!(state.modal, ModalDialog::None);
        assert_eq!(state.shares.len(), 1);
        assert!(state.ignored_folders.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn test_delete_share_ignores_unrelated_buttons() {
    let (dir, repo, mut state) = setup("confirm-delete-other");
    state.modal = ModalDialog::ConfirmDeleteShare { share_idx: 0 };
    press(&mut state, &repo, BTN_START);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 }
    );
    assert_eq!(state.shares.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_delete_share_cross_removes_it_and_declines_its_folder() {
    let (dir, repo, mut state) = setup("confirm-delete-cross");
    state.modal = ModalDialog::ConfirmDeleteShare { share_idx: 0 };
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.shares.is_empty());
    assert!(matches!(
        state.toast_msg,
        Some((UiText::ShareDeleted { .. }, _))
    ));
    assert!(state.is_folder_ignored("saves", &PEER));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_revert_on_a_read_write_share_only_returns_to_manage_devices() {
    let (dir, repo, mut state) = setup("confirm-revert-rw");
    state.modal = ModalDialog::ConfirmRevertShare { share_idx: 0 };
    let cmd = press(&mut state, &repo, BTN_CROSS);
    assert_eq!(cmd, UserCommand::None);
    assert_eq!(
        state.modal,
        ModalDialog::ManageDevices {
            share_idx: 0,
            selected_dev_idx: 0
        }
    );
    assert!(state.pending_reverts.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_revert_cross_queues_the_revert_and_starts_sync_when_idle() {
    let (dir, repo, mut state) = setup("confirm-revert-cross");
    ShareModeService::toggle(&mut state, &repo, 0);
    assert_eq!(state.shares[0].mode, ShareMode::ReadOnly);
    state.modal = ModalDialog::ConfirmRevertShare { share_idx: 0 };
    let cmd = press(&mut state, &repo, BTN_CROSS);
    assert_eq!(cmd, UserCommand::StartSync);
    assert_eq!(state.pending_reverts, vec!["saves".to_owned()]);
    assert!(matches!(
        state.modal,
        ModalDialog::ManageDevices { share_idx: 0, .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_revert_cross_does_not_restart_sync_while_connected() {
    let (dir, repo, mut state) = setup("confirm-revert-connected");
    ShareModeService::toggle(&mut state, &repo, 0);
    state.session_connected = true;
    state.modal = ModalDialog::ConfirmRevertShare { share_idx: 0 };
    assert_eq!(press(&mut state, &repo, BTN_CROSS), UserCommand::None);
    assert_eq!(state.pending_reverts, vec!["saves".to_owned()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_revert_cancel_buttons_return_without_queuing() {
    for cancel in [BTN_CIRCLE, BTN_START, BTN_SQUARE, BTN_TRIANGLE] {
        let (dir, repo, mut state) = setup("confirm-revert-cancel");
        ShareModeService::toggle(&mut state, &repo, 0);
        state.modal = ModalDialog::ConfirmRevertShare { share_idx: 0 };
        assert_eq!(press(&mut state, &repo, cancel), UserCommand::None);
        assert!(matches!(
            state.modal,
            ModalDialog::ManageDevices { share_idx: 0, .. }
        ));
        assert!(state.pending_reverts.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn test_revert_ignores_unrelated_buttons() {
    let (dir, repo, mut state) = setup("confirm-revert-other");
    ShareModeService::toggle(&mut state, &repo, 0);
    state.modal = ModalDialog::ConfirmRevertShare { share_idx: 0 };
    press(&mut state, &repo, BTN_R);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmRevertShare { share_idx: 0 }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_quit_cross_quits_and_other_answers_close_the_dialog() {
    let dir = temp_test_dir("confirm-quit");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.modal = ModalDialog::ConfirmQuit;
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);
    assert_eq!(press(&mut state, &repo, BTN_CROSS), UserCommand::QuitApp);

    for cancel in [BTN_CIRCLE, BTN_START, BTN_SQUARE, BTN_TRIANGLE] {
        state.modal = ModalDialog::ConfirmQuit;
        assert_eq!(press(&mut state, &repo, cancel), UserCommand::None);
        assert_eq!(state.modal, ModalDialog::None);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_show_device_id_routes_square_and_r_and_closes_on_cancel() {
    let dir = temp_test_dir("show-device-id");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    state.modal = ModalDialog::ShowDeviceId;
    let cmd = press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(cmd, UserCommand::OpenSystemKeyboard);
    assert_eq!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: None,
            keypad_idx: 0
        }
    );

    state.modal = ModalDialog::ShowDeviceId;
    press(&mut state, &repo, BTN_R);
    assert_eq!(
        state.modal,
        ModalDialog::ScanDeviceQr {
            return_share_idx: None
        }
    );

    for close in [BTN_CIRCLE, BTN_CROSS, BTN_TRIANGLE] {
        state.modal = ModalDialog::ShowDeviceId;
        press(&mut state, &repo, close);
        assert_eq!(state.modal, ModalDialog::None);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
