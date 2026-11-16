//! `FolderBrowser` (`Files` tab): the rows listed for the browsed directory, the selection kept
//! across refreshes, walking up to the parent (`sceIo` device roots included), and the rows of
//! the `CreateNewShare` presets.

mod common;

use common::temp_test_dir;
use vita_syncthing_app::application::{FolderBrowser, ShareService};
use vita_syncthing_app::domain::{ActiveTab, AppState, PickerMode};
use vita_syncthing_app::infrastructure::ConfigRepository;

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    (dir, repo, state)
}

#[test]
fn change_target_rows_offer_the_default_folder_of_the_selected_share() {
    let (dir, repo, mut state) = setup("browser-default-folder");
    ShareService::create_share(&mut state, &repo, "docs", "Docs", &format!("{}/docs", dir));
    state.selected_share_idx = 0;
    state.picker.mode = PickerMode::ChangeShareTarget;
    state.picker.current_dir = format!("{}/sub", dir);

    FolderBrowser::refresh(&mut state, &repo);
    // The first row keeps the current target; the default folder is the one of the share's ID.
    assert_eq!(state.picker.items[0].path, format!("{}/docs", dir));
    assert!(state
        .picker
        .items
        .iter()
        .any(|i| i.path == repo.default_share_dir("docs")));
    assert!(!state
        .picker
        .items
        .iter()
        .any(|i| i.path == repo.default_share_dir("sub")));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn subdirectories_are_joined_to_the_browsed_directory_with_one_separator() {
    let (dir, repo, mut state) = setup("browser-subdirs");
    std::fs::create_dir_all(format!("{}/photos", dir)).unwrap();
    state.picker.mode = PickerMode::ChangeShareTarget;

    for current in [dir.clone(), format!("{}/", dir)] {
        state.picker.current_dir = current;
        FolderBrowser::refresh(&mut state, &repo);
        let sub = state
            .picker
            .items
            .iter()
            .find(|i| !i.is_preset)
            .expect("the subdirectory is listed");
        assert_eq!(sub.path, format!("{}/photos", dir));
        assert!(!sub.is_preset);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refresh_keeps_the_selected_row_when_it_still_exists() {
    let (dir, repo, mut state) = setup("browser-selection");
    state.picker.mode = PickerMode::ChangeShareTarget;
    state.picker.current_dir = format!("{}/empty", dir);
    state.picker.selected_idx = 1;
    FolderBrowser::refresh(&mut state, &repo);
    assert!(state.picker.items.len() >= 2);
    assert_eq!(state.picker.selected_idx, 1);

    // A selection past the end of the new list goes back to the first row.
    state.picker.selected_idx = 500;
    FolderBrowser::refresh(&mut state, &repo);
    assert_eq!(state.picker.selected_idx, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn navigate_up_walks_one_level_then_leaves_the_tab_at_a_device_root() {
    let (dir, repo, mut state) = setup("browser-up");
    state.picker.mode = PickerMode::ChangeShareTarget;
    state.active_tab = ActiveTab::FolderPicker;

    // A sceIo-style path: `ux0:data` -> `ux0:`, then `ux0:` is a root.
    state.picker.current_dir = "ux0:data".to_owned();
    state.picker.selected_idx = 3;
    FolderBrowser::navigate_up(&mut state, &repo);
    assert_eq!(state.picker.current_dir, "ux0:");
    assert_eq!(state.picker.selected_idx, 0);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    FolderBrowser::navigate_up(&mut state, &repo);
    assert_eq!(state.active_tab, ActiveTab::Dashboard);
    assert_eq!(state.picker.current_dir, "ux0:");

    // A POSIX path: `/home` -> `/`, and `/` is a root too.
    state.active_tab = ActiveTab::FolderPicker;
    state.picker.current_dir = "/home".to_owned();
    FolderBrowser::navigate_up(&mut state, &repo);
    assert_eq!(state.picker.current_dir, "/");
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    FolderBrowser::navigate_up(&mut state, &repo);
    assert_eq!(state.active_tab, ActiveTab::Dashboard);
    assert_eq!(state.picker.current_dir, "/");

    // A trailing slash does not change the parent.
    state.active_tab = ActiveTab::FolderPicker;
    state.picker.current_dir = "/home/".to_owned();
    FolderBrowser::navigate_up(&mut state, &repo);
    assert_eq!(state.picker.current_dir, "/");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_new_share_root_lists_only_the_presets_not_shared_yet() {
    let (dir, repo, mut state) = setup("browser-presets");
    state.picker.mode = PickerMode::CreateNewShare;
    state.picker.current_dir = "ux0:".to_owned();
    // `screenshots` is already shared (same folder ID): its preset row must disappear.
    ShareService::create_share(&mut state, &repo, "screenshots", "Shots", &format!("{}/s", dir));

    FolderBrowser::refresh(&mut state, &repo);
    let templates: Vec<usize> = state.picker.items.iter().filter_map(|i| i.template).collect();
    assert!(!templates.is_empty());
    assert!(state
        .picker
        .items
        .iter()
        .all(|i| i.template.is_none() || i.path != "ux0:picture/SCREENSHOT"));
    assert!(state.picker.items.iter().all(|i| i.is_preset));

    // Inside a subdirectory the presets are not offered.
    state.picker.current_dir = format!("{}/inside", dir);
    FolderBrowser::refresh(&mut state, &repo);
    assert!(state.picker.items.iter().all(|i| i.template.is_none()));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_already_targeted_by_another_share_is_marked_shared() {
    let (dir, repo, mut state) = setup("browser-shared-mark");
    std::fs::create_dir_all(format!("{}/music", dir)).unwrap();
    ShareService::create_share(&mut state, &repo, "a", "A", &format!("{}/music", dir));
    ShareService::create_share(&mut state, &repo, "b", "B", &format!("{}/other", dir));

    state.picker.mode = PickerMode::ChangeShareTarget;
    state.selected_share_idx = 1;
    state.picker.current_dir = dir.clone();
    FolderBrowser::refresh(&mut state, &repo);
    let music = state
        .picker
        .items
        .iter()
        .find(|i| i.path == format!("{}/music", dir))
        .expect("subfolder listed");
    assert!(music.is_shared);

    // The selected share may keep its own folder.
    state.selected_share_idx = 0;
    state.picker.mode = PickerMode::ChangeShareTarget;
    FolderBrowser::refresh(&mut state, &repo);
    let music = state
        .picker
        .items
        .iter()
        .find(|i| i.path == format!("{}/music", dir))
        .expect("subfolder listed");
    assert!(!music.is_shared);

    let _ = std::fs::remove_dir_all(&dir);
}
