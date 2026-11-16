//! `Files` page controller (`FilesController`): D-pad focus over the `Up` header button and the
//! folder rows, scrolling by touch drag, and stale row indexes ignored.

mod common;

use common::{draw, feed, press, rect_of, rects_of, temp_test_dir};
use vita_syncthing_app::application::InputController;
use vita_syncthing_app::domain::{ActiveTab, AppState, NavFocus, PickerMode, UiAction};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, TouchDrag, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP,
};

/// Row index of the item whose path ends with `suffix` (presets may be listed before the folders).
fn index_of(state: &AppState, suffix: &str) -> usize {
    state
        .picker
        .items
        .iter()
        .position(|item| item.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("no row ending with {suffix}"))
}

/// `Files` tab browsing `dir`, which holds `folders` subfolders named `f00`, `f01`, ...
fn setup(name: &str, folders: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    for i in 0..folders {
        std::fs::create_dir_all(format!("{dir}/f{i:02}")).unwrap();
    }
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.active_tab = ActiveTab::FolderPicker;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    state.picker.mode = PickerMode::ChangeShareTarget;
    state.picker.current_dir = dir.clone();
    vita_syncthing_app::application::FolderBrowser::refresh(&mut state, &repo);
    (dir, repo, state)
}

#[test]
fn test_down_moves_the_row_selection_and_stops_at_the_last_row() {
    let (dir, repo, mut state) = setup("files-down", 3);
    let last = state.picker.items.len() - 1;
    assert!(last >= 2);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.picker.selected_idx, 1);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.picker.selected_idx, 2);
    for _ in 0..last + 2 {
        press(&mut state, &repo, BTN_DOWN);
    }
    // Last row: Down stays on it.
    assert_eq!(state.picker.selected_idx, last);
    press(&mut state, &repo, BTN_UP);
    assert_eq!(state.picker.selected_idx, last - 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_up_from_the_first_row_focuses_the_up_button_and_down_returns_to_it() {
    let (dir, repo, mut state) = setup("files-up-header", 2);
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::FolderPicker));
    assert_eq!(state.picker.selected_idx, 0);
    // Up on the header does nothing more.
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::FolderPicker));
    // Down leaves the header for the first row, which keeps its selection.
    press(&mut state, &repo, BTN_DOWN);
    assert!(!state.is_header_focused(ActiveTab::FolderPicker));
    assert_eq!(state.picker.selected_idx, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_right_on_the_header_does_not_open_a_folder() {
    let (dir, repo, mut state) = setup("files-right-header", 2);
    press(&mut state, &repo, BTN_UP);
    let before = state.picker.current_dir.clone();
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.picker.current_dir, before);
    assert!(state.is_header_focused(ActiveTab::FolderPicker));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_right_opens_the_focused_row_and_cross_on_the_up_button_goes_back() {
    let (dir, repo, mut state) = setup("files-open-up", 2);
    state.picker.selected_idx = index_of(&state, "/f01");
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.picker.current_dir, format!("{dir}/f01"));
    // Up from the first row of the subfolder, Cross on the header button.
    press(&mut state, &repo, BTN_UP);
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_header_focused(ActiveTab::FolderPicker));
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.picker.current_dir, dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_left_on_the_files_page_returns_to_the_sidebar() {
    let (dir, repo, mut state) = setup("files-left", 2);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_selection_keeps_the_scroll_window_on_the_selected_row() {
    let (dir, repo, mut state) = setup("files-window", 12);
    let rows = state.picker.items.len();
    for _ in 0..rows {
        press(&mut state, &repo, BTN_DOWN);
    }
    assert_eq!(state.picker.selected_idx, rows - 1);
    let visible = vita_syncthing_app::domain::BROWSER_ROWS;
    assert_eq!(state.picker.top_idx, rows - visible);
    press(&mut state, &repo, BTN_UP);
    assert!(state.picker.selected_idx >= state.picker.top_idx);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_dragging_over_the_list_scrolls_it_without_selecting() {
    let (dir, repo, mut state) = setup("files-drag", 12);
    draw(&mut state);
    let area = state.hit_map.scroll_areas()[0];
    let start = (
        (area.rect.x + area.rect.w / 2) as usize,
        (area.rect.y + area.rect.h / 2) as usize,
    );
    // Finger moved up by two rows: the list scrolls by two rows.
    let drag = |total_dy: i32, started: bool| InputFrame {
        touch_drag: Some(TouchDrag {
            start,
            total_dy,
            started,
        }),
        touch_pos: Some(start),
        any_activity: true,
        ..Default::default()
    };
    feed(&mut state, &repo, drag(-2 * area.step, true));
    assert_eq!(state.picker.top_idx, 2);
    assert_eq!(state.picker.selected_idx, 2);
    assert_eq!(state.picker.current_dir, dir);
    // Finger back down by one row.
    feed(&mut state, &repo, drag(-area.step, false));
    assert_eq!(state.picker.top_idx, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_row_index_of_a_stale_frame_is_ignored() {
    let (dir, repo, mut state) = setup("files-stale", 3);
    let rect = rect_of(&mut state, UiAction::PickerSelect(2));
    // The listing shrinks after the frame was drawn: the old row no longer exists.
    state.picker.items.truncate(1);
    state.picker.selected_idx = 0;
    let before = state.picker.current_dir.clone();
    InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_tap: Some((
                (rect.x + rect.w / 2) as usize,
                (rect.y + rect.h / 2) as usize,
            )),
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.picker.selected_idx, 0);
    assert_eq!(state.picker.current_dir, before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_tapping_a_row_selects_it_and_the_open_chip_enters_it() {
    let (dir, repo, mut state) = setup("files-tap", 3);
    // A plain folder row: presets are shares to create, not folders to enter.
    let idx = state
        .picker
        .items
        .iter()
        .position(|item| !item.is_preset && item.path.starts_with(&dir) && item.path != dir)
        .unwrap();
    let target = state.picker.items[idx].path.clone();
    // Scroll the window so that the row is on screen.
    state.picker.selected_idx = idx;
    state.picker.clamp_scroll();
    let rows = rects_of(&mut state, UiAction::PickerSelect(idx));
    assert!(!rows.is_empty());
    common::tap_rect(&mut state, &repo, rows[0]);
    assert_eq!(state.picker.selected_idx, idx);
    assert_eq!(state.picker.current_dir, dir);
    // Once selected, the row's `Open` chip enters it.
    common::tap_action(&mut state, &repo, UiAction::PickerOpen(idx));
    assert_eq!(state.picker.current_dir, target);
    let _ = std::fs::remove_dir_all(&dir);
}
