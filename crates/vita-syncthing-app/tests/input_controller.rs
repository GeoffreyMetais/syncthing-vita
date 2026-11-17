//! `InputController` global routing: an active OLED screensaver swallows input and wakes up on a
//! button press or touch contact.

mod common;

use common::{press, tap_action, temp_test_dir};
use vita_syncthing_app::application::{InputController, ShareService, UserCommand};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, ModalDialog, NavFocus, PickerMode, UiAction,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_R, BTN_RIGHT,
    BTN_SELECT, BTN_SQUARE, BTN_START, BTN_TRIANGLE,
};

#[test]
fn test_screensaver_wakes_on_press_or_touch_only() {
    let dir = temp_test_dir("saver-wake");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");

    state.oled_saver_active = true;
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
    // So does any button, `SELECT` included (it no longer starts the screensaver).
    state.oled_saver_active = true;
    press(&mut state, &repo, BTN_SELECT);
    assert!(!state.oled_saver_active);
    press(&mut state, &repo, BTN_SELECT);
    assert!(!state.oled_saver_active);
    state.oled_saver_active = true;
    press(&mut state, &repo, BTN_CIRCLE);
    assert!(!state.oled_saver_active);
}

/// State on the `Overview` with `shares` shares created (the page content focused).
fn overview(name: &str, shares: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    for i in 0..shares {
        let id = format!("s{i}");
        ShareService::create_share(&mut state, &repo, &id, &id, &format!("{dir}/{id}"));
    }
    state.selected_share_idx = 0;
    state.active_tab = ActiveTab::Dashboard;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    (dir, repo, state)
}

#[test]
fn test_start_opens_the_quit_confirmation_from_anywhere() {
    let (dir, repo, mut state) = overview("input-start", 0);
    assert_eq!(press(&mut state, &repo, BTN_START), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_l_and_r_cycle_the_tabs_in_both_directions() {
    let (dir, repo, mut state) = overview("input-lr", 0);
    press(&mut state, &repo, BTN_L);
    assert_eq!(
        state.active_tab,
        ActiveTab::Settings,
        "L from the first tab wraps"
    );
    press(&mut state, &repo, BTN_R);
    assert_eq!(
        state.active_tab,
        ActiveTab::Dashboard,
        "R from the last tab wraps"
    );
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_switching_to_files_opens_the_share_creation_browser() {
    let (dir, repo, mut state) = overview("input-files-tab", 0);
    state.picker.mode = PickerMode::AcceptedShareTarget;
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(state.picker.mode, PickerMode::CreateNewShare);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_switching_tab_resets_the_focus_of_the_new_page() {
    let (dir, repo, mut state) = overview("input-reset-nav", 0);
    press(&mut state, &repo, BTN_R);
    press(&mut state, &repo, BTN_R);
    // Settings: a card other than the first one was focused, then the user leaves the page.
    state.nav.row = 3;
    state.nav.col = 1;
    press(&mut state, &repo, BTN_L);
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.active_tab, ActiveTab::Settings);
    assert_eq!(state.nav.row, 0);
    assert_eq!(state.nav.col, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_circle_on_overview_without_a_share_row_opens_the_device_id() {
    let (dir, repo, mut state) = overview("input-circle", 2);
    state.nav.header = true;
    assert_eq!(press(&mut state, &repo, BTN_CIRCLE), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::ShowDeviceId);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_square_deletes_the_selected_share_from_any_focus_on_the_overview() {
    // `Delete` is a page-level hint of the Overview: Square acts on the selected share.
    let (dir, repo, mut state) = overview("input-square-row", 2);
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 }
    );
    state.modal = ModalDialog::None;
    // Focus the second share row: Square deletes that one.
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 1);
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 1 }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_triangle_is_new_share_on_the_header_and_on_a_share_row() {
    let (dir, repo, mut state) = overview("input-triangle", 1);
    // Header focused: Triangle is New Share.
    press(&mut state, &repo, BTN_TRIANGLE);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;
    // Share row focused: still New Share (a share's folder cannot be changed).
    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_share_row_focused());
    press(&mut state, &repo, BTN_TRIANGLE);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    assert_eq!(state.active_tab, ActiveTab::Dashboard);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_on_a_focused_share_row_button_runs_that_button() {
    let (dir, repo, mut state) = overview("input-cross-row", 1);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_sync_card_starts_a_sync_only_when_none_is_running() {
    let (dir, repo, mut state) = overview("input-sync-card", 0);
    common::rect_of(&mut state, UiAction::StartSync);
    assert_eq!(
        tap_action(&mut state, &repo, UiAction::StartSync),
        UserCommand::StartSync
    );
    state.sync_in_progress = true;
    assert_eq!(
        tap_action(&mut state, &repo, UiAction::StartSync),
        UserCommand::None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_outside_every_element_and_non_scroll_drag_change_nothing() {
    let (dir, repo, mut state) = overview("input-touch-nothing", 1);
    let before = state.nav.clone();
    common::draw(&mut state);
    // Top-left corner of the screen: no element is declared there.
    InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_tap: Some((1, 1)),
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.nav, before);
    assert_eq!(state.modal, ModalDialog::None);
    // A drag that does not start on a scrollable list is ignored.
    InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_drag: Some(vita_syncthing_app::infrastructure::TouchDrag {
                start: (1, 1),
                total_dy: -50,
                started: true,
            }),
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.nav, before);
    let _ = std::fs::remove_dir_all(&dir);
}
