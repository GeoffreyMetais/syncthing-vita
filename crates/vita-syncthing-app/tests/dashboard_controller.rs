//! `Overview` D-pad focus (`DashboardController`): the device card header row, then one row per
//! share with its `[Devices, Delete, Folder]` buttons, with the edges of the list.

mod common;

use common::{press, temp_test_dir};
use vita_syncthing_app::application::{ShareService, UserCommand};
use vita_syncthing_app::domain::{ActiveTab, AppState, ModalDialog, NavFocus};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP,
};

fn setup(name: &str, shares: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    for i in 0..shares {
        let id = format!("s{i}");
        let path = format!("{dir}/{id}");
        ShareService::create_share(&mut state, &repo, &id, &id, &path);
    }
    // Creating a share selects it: start from the first row of the list.
    state.selected_share_idx = 0;
    state.active_tab = ActiveTab::Dashboard;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    (dir, repo, state)
}

#[test]
fn test_overview_opens_on_the_header_row() {
    let (dir, _repo, state) = setup("dash-open", 2);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    assert_eq!(state.selected_share_idx, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_down_walks_from_header_to_rows_and_stops_at_the_last_row() {
    let (dir, repo, mut state) = setup("dash-down", 3);
    press(&mut state, &repo, BTN_DOWN);
    assert!(!state.is_header_focused(ActiveTab::Dashboard));
    assert_eq!(state.selected_share_idx, 0);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 1);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 2);
    // Last row: Down stays on it.
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 2);
    assert!(!state.is_header_focused(ActiveTab::Dashboard));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_up_walks_back_to_the_header_and_stops_there() {
    let (dir, repo, mut state) = setup("dash-up", 2);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 1);
    press(&mut state, &repo, BTN_UP);
    assert_eq!(state.selected_share_idx, 0);
    assert!(!state.is_header_focused(ActiveTab::Dashboard));
    // First row: Up goes to the device card header.
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    // Header: Up has nowhere to go and does not touch the share selection.
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    assert_eq!(state.selected_share_idx, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_header_row_has_two_buttons_and_left_returns_to_sidebar() {
    let (dir, repo, mut state) = setup("dash-header-cols", 1);
    // Right on the header moves to New Share, and stops there (two buttons).
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 1);
    assert_eq!(press(&mut state, &repo, BTN_CROSS), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;
    // Left back to QR / ID, then Left leaves for the sidebar.
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.col, 0);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_share_row_has_three_buttons_and_right_stops_on_folder() {
    let (dir, repo, mut state) = setup("dash-row-cols", 1);
    press(&mut state, &repo, BTN_DOWN);
    for _ in 0..5 {
        press(&mut state, &repo, BTN_RIGHT);
    }
    // Devices (0), Delete (1), Folder (2): Right never goes past Folder.
    assert_eq!(state.nav.col, 2);
    assert_eq!(state.focused_action(ActiveTab::Dashboard), Some(2));
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.col, 1);
    press(&mut state, &repo, BTN_LEFT);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.col, 0);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_header_column_is_clamped_when_moving_from_a_share_row() {
    let (dir, repo, mut state) = setup("dash-clamp-col", 1);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 2);
    // Up to the header: the column is clamped to the header's last button.
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    assert_eq!(state.nav.col, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_without_shares_the_header_stays_focused() {
    let (dir, repo, mut state) = setup("dash-empty", 0);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::Dashboard));
    assert_eq!(state.selected_share_idx, 0);
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_selection_is_clamped_after_the_share_list_shrinks() {
    let (dir, repo, mut state) = setup("dash-shrink", 3);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 2);
    state.shares.truncate(1);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 0);
    let _ = std::fs::remove_dir_all(&dir);
}
