//! `ManageDevices` modal input (`modal_input/manage_devices.rs`): device row cursor and touch,
//! device linking, closing with discard of a share still without a device, the pad shortcuts
//! (`Square`, `R`, `Select`, `L`, `Start`, `Triangle`) and the LAN scan without known devices.

mod common;

use common::{press, rect_of, tap, tap_rect, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, ShareCreation, ShareService, UserCommand};
use vita_syncthing_app::domain::{AppState, ModalDialog, ShareMode, UiAction, UiText};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_LEFT, BTN_R, BTN_RIGHT,
    BTN_SELECT, BTN_SQUARE, BTN_START, BTN_TRIANGLE, BTN_UP,
};

const MAC: DeviceId = DeviceId([0x51; 32]);
const NAS: DeviceId = DeviceId([0x52; 32]);

/// One share `Saves` (index 0) and two known devices, neither linked yet. The modal is open on
/// the share with the first device row selected.
fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    DeviceRegistry::upsert_device(&mut state, &repo, MAC, "MacBook", "192.168.1.10:22000");
    DeviceRegistry::upsert_device(&mut state, &repo, NAS, "NAS", "192.168.1.11:22000");
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    state.modal = ModalDialog::ManageDevices {
        share_idx: 0,
        selected_dev_idx: 0,
    };
    (dir, repo, state)
}

fn selected_dev(state: &AppState) -> usize {
    match state.modal {
        ModalDialog::ManageDevices {
            selected_dev_idx, ..
        } => selected_dev_idx,
        other => panic!("expected ManageDevices, got {other:?}"),
    }
}

fn linked(state: &AppState, device: DeviceId) -> bool {
    state.shares[0].shared_devices.contains(&device)
}

#[test]
fn test_device_cursor_stays_within_the_known_devices() {
    let (dir, repo, mut state) = setup("manage-cursor");
    press(&mut state, &repo, BTN_UP);
    assert_eq!(selected_dev(&state), 0);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(selected_dev(&state), 1);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(selected_dev(&state), 1);
    press(&mut state, &repo, BTN_UP);
    assert_eq!(selected_dev(&state), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_links_then_unlinks_the_selected_device() {
    let (dir, repo, mut state) = setup("manage-cross");
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.devices[selected_dev(&state)].device_id, NAS);

    press(&mut state, &repo, BTN_CROSS);
    assert!(linked(&state, NAS));
    assert!(!linked(&state, MAC));
    assert!(matches!(
        state.toast_msg,
        Some((UiText::DeviceLinked { .. }, _))
    ));
    assert!(state
        .logs
        .iter()
        .any(|l| l.contains("Added NAS to 'Saves'")));

    press(&mut state, &repo, BTN_CROSS);
    assert!(!linked(&state, NAS));
    assert!(matches!(
        state.toast_msg,
        Some((UiText::DeviceUnlinked { .. }, _))
    ));
    assert!(state
        .logs
        .iter()
        .any(|l| l.contains("Removed NAS from 'Saves'")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_touching_a_device_row_links_that_device() {
    let (dir, repo, mut state) = setup("manage-row-touch");
    let row = rect_of(&mut state, UiAction::ModalRow(1));
    tap_rect(&mut state, &repo, row);
    assert_eq!(selected_dev(&state), 1);
    assert!(linked(&state, NAS));
    assert!(!linked(&state, MAC));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_circle_keeps_a_share_with_a_linked_device() {
    let (dir, repo, mut state) = setup("manage-close-linked");
    ShareCreation::begin(&mut state, &repo, "docs", "Docs", "/tmp/docs");
    let idx = state.shares.len() - 1;
    state.modal = ModalDialog::ManageDevices {
        share_idx: idx,
        selected_dev_idx: 0,
    };
    press(&mut state, &repo, BTN_CROSS);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.shares.iter().any(|s| s.folder_id == "docs"));
    assert!(state.pending_share_creation.is_none());
    assert!(matches!(
        state.toast_msg,
        Some((UiText::ShareCreated { .. }, _))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_circle_discards_a_new_share_without_a_linked_device() {
    let (dir, repo, mut state) = setup("manage-close-unlinked");
    let before = state.shares.len();
    ShareCreation::begin(&mut state, &repo, "docs", "Docs", "/tmp/docs");
    assert_eq!(state.shares.len(), before + 1);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.shares.len(), before);
    assert!(state.shares.iter().all(|s| s.folder_id != "docs"));
    assert!(matches!(
        state.toast_msg,
        Some((UiText::ShareNotCreated { .. }, _))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_square_opens_add_by_id_and_r_opens_the_scanner_for_this_share() {
    let (dir, repo, mut state) = setup("manage-square-r");
    let cmd = press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(cmd, UserCommand::OpenSystemKeyboard);
    assert_eq!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: Some(0),
            keypad_idx: 0
        }
    );
    state.modal = ModalDialog::ManageDevices {
        share_idx: 0,
        selected_dev_idx: 0,
    };
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
fn test_select_pauses_and_resumes_the_share() {
    let (dir, repo, mut state) = setup("manage-select");
    assert!(state.shares[0].enabled);
    press(&mut state, &repo, BTN_SELECT);
    assert!(!state.shares[0].enabled);
    assert!(state.config_dirty);
    press(&mut state, &repo, BTN_SELECT);
    assert!(state.shares[0].enabled);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_l_switches_between_read_write_and_read_only() {
    let (dir, repo, mut state) = setup("manage-l");
    assert_eq!(state.shares[0].mode, ShareMode::ReadWrite);
    press(&mut state, &repo, BTN_L);
    assert_eq!(state.shares[0].mode, ShareMode::ReadOnly);
    press(&mut state, &repo, BTN_L);
    assert_eq!(state.shares[0].mode, ShareMode::ReadWrite);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_start_asks_to_revert_only_a_read_only_share() {
    let (dir, repo, mut state) = setup("manage-start");
    press(&mut state, &repo, BTN_START);
    assert!(matches!(
        state.modal,
        ModalDialog::ManageDevices { share_idx: 0, .. }
    ));
    press(&mut state, &repo, BTN_L);
    press(&mut state, &repo, BTN_START);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmRevertShare { share_idx: 0 }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_triangle_scans_the_lan_only_without_known_devices_and_no_sync() {
    let (dir, repo, mut state) = setup("manage-triangle");
    assert_eq!(press(&mut state, &repo, BTN_TRIANGLE), UserCommand::None);

    let dir2 = temp_test_dir("manage-triangle-empty");
    let repo2 = ConfigRepository::new(&dir2);
    let mut empty = AppState::new(Vec::new(), Vec::new(), &dir2);
    ShareService::create_share(&mut empty, &repo2, "saves", "Saves", "/tmp/saves");
    empty.modal = ModalDialog::ManageDevices {
        share_idx: 0,
        selected_dev_idx: 0,
    };
    assert_eq!(
        press(&mut empty, &repo2, BTN_TRIANGLE),
        UserCommand::ScanLan
    );
    assert!(matches!(empty.modal, ModalDialog::ManageDevices { .. }));

    empty.sync_in_progress = true;
    assert_eq!(press(&mut empty, &repo2, BTN_TRIANGLE), UserCommand::None);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn test_touching_the_dialog_body_does_nothing_and_outside_closes_it() {
    let (dir, repo, mut state) = setup("manage-outside");
    let body = common::rects_of(&mut state, UiAction::Inert)[0];
    let cmd = tap_rect(&mut state, &repo, body);
    assert_eq!(cmd, UserCommand::None);
    assert!(!linked(&state, MAC) && !linked(&state, NAS));
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));

    // A touch outside the card closes the dialog like `Circle` (new shares are checked).
    tap(&mut state, &repo, 5, 5);
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_unused_pad_buttons_keep_the_dialog_open() {
    let (dir, repo, mut state) = setup("manage-unused");
    press(&mut state, &repo, BTN_LEFT);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(
        state.modal,
        ModalDialog::ManageDevices {
            share_idx: 0,
            selected_dev_idx: 0
        }
    );
    let _ = std::fs::remove_dir_all(&dir);
}
