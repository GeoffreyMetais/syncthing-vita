//! D-pad navigation: sidebar tabs <-> page elements, rows and action buttons, `Cross` activation.

mod common;

use common::{press, temp_test_dir};
use vita_syncthing_app::application::{ShareService, UserCommand};
use vita_syncthing_app::domain::{ActiveTab, AppState, ModalDialog, NavFocus};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP,
};

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    (dir, repo, state)
}

#[test]
fn test_dashboard_buttons_then_sidebar_tabs() {
    let (dir, repo, mut state) = setup("dpad-dashboard");

    // Initially focus is on Sidebar (Tab). Right enters page content.
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.focus, NavFocus::Content);

    // The page opens on the device card buttons: col 0 is QR / ID, Right moves to New Share
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(press(&mut state, &repo, BTN_CROSS), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;
    press(&mut state, &repo, BTN_LEFT);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::ShowDeviceId);
    state.modal = ModalDialog::None;

    // Left on the first button moves the focus to the sidebar, where Up/Down switch tabs
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.active_tab, ActiveTab::Settings);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.active_tab, ActiveTab::Settings);
    press(&mut state, &repo, BTN_UP);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);

    // Right (or Cross) goes back into the page
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.focus, NavFocus::Content);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_overview_share_rows_buttons_and_device_card() {
    let (dir, repo, mut state) = setup("dpad-shares");
    ShareService::create_share(&mut state, &repo, "a", "A", &format!("{}/a", dir));
    ShareService::create_share(&mut state, &repo, "b", "B", &format!("{}/b", dir));
    state.selected_share_idx = 0;
    state.active_tab = ActiveTab::Dashboard;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    assert!(state.is_header_focused(ActiveTab::Dashboard));

    // Down leaves the device card for the first share, then selects the second one;
    // Right moves to its Delete button
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 0);
    assert!(!state.is_header_focused(ActiveTab::Dashboard));
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 1);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.focused_action(ActiveTab::Dashboard), Some(1));
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 1 }
    );
    state.modal = ModalDialog::None;

    // Right reaches the Folder button, which has no face button shortcut
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.focused_action(ActiveTab::Dashboard), Some(2));
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    state.active_tab = ActiveTab::Dashboard;
    state.nav.header = false;
    state.nav.col = 1;

    // Up twice reaches the device card, on its last button (New Share)
    press(&mut state, &repo, BTN_UP);
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;

    // Left from its first button goes to the sidebar
    press(&mut state, &repo, BTN_LEFT);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_files_rows_open_and_up_button() {
    let (dir, repo, mut state) = setup("dpad-files");
    std::fs::create_dir_all(format!("{}/saves/sub", dir)).unwrap();

    // Sidebar -> Files tab -> page
    press(&mut state, &repo, BTN_LEFT);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(state.nav.focus, NavFocus::Content);

    // Down to "saves/", Right opens it
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.picker_current_dir, format!("{}/saves", dir));

    // Up from the first row focuses the "Up" button; Cross goes back up
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::FolderPicker));
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.picker_current_dir, dir);

    let _ = std::fs::remove_dir_all(&dir);
}
