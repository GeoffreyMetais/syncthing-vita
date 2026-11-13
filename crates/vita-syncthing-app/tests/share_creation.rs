//! `ShareCreation`: a new share is only kept once a device is linked in `ManageDevices`.

mod common;

use common::{press, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{ShareCreation, ShareService, UserCommand};
use vita_syncthing_app::domain::{AppState, ModalDialog};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_TRIANGLE};

#[test]
fn test_share_without_linked_device_is_discarded() {
    let dir = temp_test_dir("creation-discard");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let dev_id = DeviceId([0x11; 32]);
    ShareService::upsert_device(&mut state, &repo, dev_id, "Laptop", "192.168.1.5:22000");

    let idx = ShareCreation::begin(&mut state, &repo, "new", "New", &format!("{}/new", dir));
    assert!(state.is_share_creation_incomplete(idx));
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));

    // `Triangle` only scans the LAN when no device is known: here it does nothing
    assert_eq!(press(&mut state, &repo, BTN_TRIANGLE), UserCommand::None);
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));

    // Closing the modal discards the share
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.shares.is_empty());
    assert!(state.pending_share_creation.is_none());
    assert!(repo.load_shares().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_share_with_linked_device_is_kept() {
    let dir = temp_test_dir("creation-keep");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let dev_id = DeviceId([0x22; 32]);
    ShareService::upsert_device(&mut state, &repo, dev_id, "Laptop", "192.168.1.5:22000");

    let idx = ShareCreation::begin(&mut state, &repo, "new", "New", &format!("{}/new", dir));
    press(&mut state, &repo, BTN_CROSS); // link the first device
    assert!(!state.is_share_creation_incomplete(idx));

    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.shares.len(), 1);
    assert_eq!(state.shares[0].shared_devices, vec![dev_id.to_hex()]);
    assert!(state.pending_share_creation.is_none());
    assert_eq!(repo.load_shares().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_reopening_existing_share_never_discards_it() {
    let dir = temp_test_dir("creation-existing");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let path = format!("{}/old", dir);
    ShareService::create_share(&mut state, &repo, "old", "Old", &path);

    ShareCreation::begin(&mut state, &repo, "old", "Old", &path);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.shares.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_selecting_a_folder_in_files_tab_starts_share_creation() {
    use vita_syncthing_app::domain::{ActiveTab, PickerMode};
    use vita_syncthing_app::infrastructure::{BTN_DOWN, BTN_R, BTN_RIGHT};

    let dir = temp_test_dir("files-tab-create");
    std::fs::create_dir_all(format!("{}/saves", dir)).unwrap();
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // Overview -> Files
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(state.picker_mode, PickerMode::CreateNewShare);
    assert_eq!(state.picker_items[1].path, format!("{}/saves", dir));

    // Right enters page content from sidebar
    press(&mut state, &repo, BTN_RIGHT);

    // Cross on the "saves/" folder opens device linking for a new pending share
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.active_tab, ActiveTab::Dashboard);
    assert_eq!(state.shares.len(), 1);
    assert_eq!(state.shares[0].folder_id, "saves");
    assert_eq!(state.shares[0].target_path, format!("{}/saves", dir));
    assert!(state.is_share_creation_incomplete(0));
    assert!(matches!(
        state.modal,
        ModalDialog::ManageDevices { share_idx: 0, .. }
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_already_shared_folders_are_not_selectable_in_files_tab() {
    use vita_syncthing_app::domain::{ActiveTab, PickerMode};
    use vita_syncthing_app::infrastructure::BTN_DOWN;

    let dir = temp_test_dir("files-tab-shared");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let saves = format!("{}/saves", dir);
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{}/", saves));

    state.active_tab = ActiveTab::FolderPicker;
    state.picker_mode = PickerMode::CreateNewShare;
    state.reset_nav_for_tab();
    state.nav.focus = vita_syncthing_app::domain::NavFocus::Content;
    ShareService::refresh_picker_items(&mut state, &repo);
    assert_eq!(state.picker_items[1].path, saves);
    assert!(state.picker_items[1].is_shared);
    assert!(!state.picker_items[0].is_shared);

    // Neither selecting nor opening the shared folder does anything
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_CROSS);
    press(&mut state, &repo, BTN_TRIANGLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(state.picker_current_dir, dir);
    assert_eq!(state.shares.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}
