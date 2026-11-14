//! `Files` tab (`ux0:` browser): `Circle` walks up the device-root path, then leaves the tab;
//! the folder list scrolls with a held `Up` / `Down` and with touch drags.

mod common;

use common::{press, temp_test_dir};
use vita_syncthing_app::application::InputGestures;
use vita_syncthing_app::domain::defaults::{KEY_REPEAT_DELAY, KEY_REPEAT_INTERVAL};
use vita_syncthing_app::domain::{ActiveTab, AppState, ScrollTarget, UiAction, BROWSER_ROWS};
use vita_syncthing_app::infrastructure::{ConfigRepository, InputFrame, BTN_CIRCLE, BTN_DOWN};

#[test]
fn test_files_tab_up_navigation_walks_device_root_paths() {
    let dir = temp_test_dir("picker-up");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:data/retroarch");
    state.active_tab = ActiveTab::FolderPicker;
    state.selected_picker_idx = 3;

    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.picker_current_dir, "ux0:data");
    assert_eq!(state.selected_picker_idx, 0);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.picker_current_dir, "ux0:");
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.active_tab, ActiveTab::Dashboard);

    let _ = std::fs::remove_dir_all(&dir);
}

/// `Files` tab on a folder holding `count` subfolders, D-pad focus on the first row.
fn files_tab_with_subfolders(name: &str, count: usize) -> (String, ConfigRepository, AppState) {
    use vita_syncthing_app::application::ShareService;
    use vita_syncthing_app::domain::{NavFocus, PickerMode};
    let dir = temp_test_dir(name);
    for i in 0..count {
        std::fs::create_dir_all(format!("{}/sub{:02}", dir, i)).unwrap();
    }
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.active_tab = ActiveTab::FolderPicker;
    state.picker_mode = PickerMode::CreateNewShare;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    ShareService::refresh_picker_items(&mut state, &repo);
    assert_eq!(state.picker_items.len(), count);
    (dir, repo, state)
}

/// Feeds `frame` through the input gestures, as the main loop does.
fn feed_gesture(
    state: &mut AppState,
    repo: &ConfigRepository,
    gestures: &mut InputGestures,
    mut frame: InputFrame,
) {
    gestures.apply(&mut frame);
    common::feed(state, repo, frame);
}

/// Drags a finger over the `Files` list from `y0` to `y1` (one pixel per frame), then lifts it.
fn drag_list(state: &mut AppState, repo: &ConfigRepository, y0: i32, y1: i32) {
    let mut gestures = InputGestures::new();
    common::draw(state);
    let area = state.hit_map.scroll_areas()[0];
    let x = (area.rect.x + area.rect.w / 2) as usize;
    let dir = if y1 >= y0 { 1 } else { -1 };
    let mut y = y0;
    loop {
        let pos = Some((x, y as usize));
        let frame = InputFrame {
            touch_pos: pos,
            touch_down: (y == y0).then_some((x, y as usize)),
            any_activity: true,
            ..Default::default()
        };
        feed_gesture(state, repo, &mut gestures, frame);
        if y == y1 {
            break;
        }
        y += dir;
    }
    feed_gesture(state, repo, &mut gestures, InputFrame::default());
}

#[test]
fn test_holding_down_scrolls_the_files_list() {
    let (dir, repo, mut state) = files_tab_with_subfolders("picker-hold", 30);
    let mut gestures = InputGestures::new();
    let held_vblanks = KEY_REPEAT_DELAY + 8 * KEY_REPEAT_INTERVAL;
    feed_gesture(
        &mut state,
        &repo,
        &mut gestures,
        InputFrame {
            pressed: BTN_DOWN,
            held: BTN_DOWN,
            ..Default::default()
        },
    );
    for _ in 0..held_vblanks {
        let frame = InputFrame {
            held: BTN_DOWN,
            any_activity: true,
            ..Default::default()
        };
        feed_gesture(&mut state, &repo, &mut gestures, frame);
    }
    // One press, then a repeat after the delay and every interval.
    assert_eq!(state.selected_picker_idx, 1 + 9);
    assert!(state.picker_top_idx > 0);
    assert!(state.selected_picker_idx < state.picker_top_idx + BROWSER_ROWS);
    assert!(state.shares.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_dragging_the_files_list_scrolls_it_without_selecting() {
    let (dir, repo, mut state) = files_tab_with_subfolders("picker-drag", 30);
    common::draw(&mut state);
    let area = state.hit_map.scroll_areas()[0];
    assert_eq!(area.target, ScrollTarget::FilesList);
    let bottom = area.rect.y + area.rect.h - 4;

    // Finger moves up by 3 rows: the content follows it.
    drag_list(&mut state, &repo, bottom, bottom - 3 * area.step - 5);
    assert_eq!(state.picker_top_idx, 3);
    assert!(state.selected_picker_idx >= 3);
    assert!(state.shares.is_empty(), "a drag is not a tap");
    assert_eq!(state.picker_current_dir, dir);

    // Dragging down further than the top stops at the first row.
    drag_list(&mut state, &repo, area.rect.y + 2, bottom);
    assert_eq!(state.picker_top_idx, 0);
    assert!(state.selected_picker_idx < BROWSER_ROWS);

    // Dragging up past the end stops on the last page.
    for _ in 0..4 {
        drag_list(&mut state, &repo, bottom, area.rect.y);
    }
    assert_eq!(state.picker_top_idx, 30 - BROWSER_ROWS);
    assert!(state.selected_picker_idx >= 30 - BROWSER_ROWS);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_tap_through_gestures_still_selects_a_row() {
    let (dir, repo, mut state) = files_tab_with_subfolders("picker-gesture-tap", 3);
    let row = common::rect_of(&mut state, UiAction::PickerSelect(1));
    let pos = ((row.x + 60) as usize, (row.y + row.h / 2) as usize);
    let mut gestures = InputGestures::new();
    let down = InputFrame {
        touch_pos: Some(pos),
        touch_down: Some(pos),
        ..Default::default()
    };
    feed_gesture(&mut state, &repo, &mut gestures, down);
    assert!(
        state.shares.is_empty(),
        "nothing happens before the finger is lifted"
    );
    feed_gesture(&mut state, &repo, &mut gestures, InputFrame::default());
    assert_eq!(state.shares.len(), 1);
    assert!(state.shares[0].target_path.ends_with("/sub01"));

    let _ = std::fs::remove_dir_all(&dir);
}
