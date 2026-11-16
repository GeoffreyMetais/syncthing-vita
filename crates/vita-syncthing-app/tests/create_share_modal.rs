//! `CreateShare` modal input (`modal_input/create_share.rs`): template cursor and touch, the
//! `Browse` entry that opens the `Files` tab, and the share created from a preset.

mod common;

use common::{press, rect_of, tap_rect, temp_test_dir};
use vita_syncthing_app::application::UserCommand;
use vita_syncthing_app::domain::{
    ActiveTab, AppState, ModalDialog, PickerMode, UiAction, BROWSE_TEMPLATE_ID, NEW_SHARE_TEMPLATES,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_UP,
};

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.modal = ModalDialog::CreateShare { selected_idx: 0 };
    (dir, repo, state)
}

fn selected(state: &AppState) -> usize {
    match state.modal {
        ModalDialog::CreateShare { selected_idx } => selected_idx,
        other => panic!("expected CreateShare, got {other:?}"),
    }
}

#[test]
fn test_template_cursor_stops_at_both_ends() {
    let (dir, repo, mut state) = setup("create-share-cursor");
    press(&mut state, &repo, BTN_UP);
    assert_eq!(selected(&state), 0);
    for _ in 0..NEW_SHARE_TEMPLATES.len() + 2 {
        press(&mut state, &repo, BTN_DOWN);
    }
    assert_eq!(selected(&state), NEW_SHARE_TEMPLATES.len() - 1);
    press(&mut state, &repo, BTN_UP);
    assert_eq!(selected(&state), NEW_SHARE_TEMPLATES.len() - 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_creates_the_preset_under_the_cursor_and_opens_its_devices() {
    let (dir, repo, mut state) = setup("create-share-cross");
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_CROSS);
    let (folder_id, label, _) = NEW_SHARE_TEMPLATES[1];
    let share = state
        .shares
        .iter()
        .find(|s| s.folder_id == folder_id)
        .expect("preset share is created");
    assert_eq!(share.label, label);
    assert_eq!(
        state.modal,
        ModalDialog::ManageDevices {
            share_idx: state
                .shares
                .iter()
                .position(|s| s.folder_id == folder_id)
                .unwrap(),
            selected_dev_idx: 0
        }
    );
    assert_eq!(state.pending_share_creation.as_deref(), Some(folder_id));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_touching_a_template_row_creates_that_preset() {
    let (dir, repo, mut state) = setup("create-share-touch");
    let rect = rect_of(&mut state, UiAction::ModalRow(2));
    tap_rect(&mut state, &repo, rect);
    let (folder_id, _, _) = NEW_SHARE_TEMPLATES[2];
    assert!(state.shares.iter().any(|s| s.folder_id == folder_id));
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_browse_entry_opens_the_folder_picker_in_create_mode() {
    let (dir, repo, mut state) = setup("create-share-browse");
    let browse = NEW_SHARE_TEMPLATES
        .iter()
        .position(|t| t.0 == BROWSE_TEMPLATE_ID)
        .expect("browse entry exists");
    state.modal = ModalDialog::CreateShare {
        selected_idx: browse,
    };
    let shares_before = state.shares.len();
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.picker.mode, PickerMode::CreateNewShare);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(state.shares.len(), shares_before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_circle_closes_without_creating_anything() {
    let (dir, repo, mut state) = setup("create-share-circle");
    let cmd = press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(cmd, UserCommand::None);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.shares.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
