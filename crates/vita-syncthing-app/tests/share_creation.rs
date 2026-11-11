//! `ShareCreation`: a new share is only kept once a device is linked in `ManageDevices`.

mod common;

use common::temp_test_dir;
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{InputController, ShareCreation, ShareService, UserCommand};
use vita_syncthing_app::domain::{AppState, ModalDialog};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_TRIANGLE,
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
fn test_share_without_linked_device_is_discarded() {
    let dir = temp_test_dir("creation-discard");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let dev_id = DeviceId([0x11; 32]);
    ShareService::upsert_device(&mut state, &repo, dev_id, "Laptop", "192.168.1.5:22000");

    let idx = ShareCreation::begin(&mut state, &repo, "new", "New", &format!("{}/new", dir));
    assert!(state.is_share_creation_incomplete(idx));
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));

    // Apply & Sync is refused while no device is linked
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
    use vita_syncthing_app::infrastructure::{BTN_DOWN, BTN_R};

    let dir = temp_test_dir("files-tab-create");
    std::fs::create_dir_all(format!("{}/saves", dir)).unwrap();
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // Dashboard -> Shares -> Files
    press(&mut state, &repo, BTN_R);
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(state.picker_mode, PickerMode::CreateNewShare);
    assert_eq!(state.picker_items[1].path, format!("{}/saves", dir));

    // Cross on the "saves/" folder opens device linking for a new pending share
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.active_tab, ActiveTab::Shares);
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
