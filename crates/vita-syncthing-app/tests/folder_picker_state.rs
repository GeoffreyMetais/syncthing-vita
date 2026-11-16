//! `FolderPicker` (`Files` tab browser state): scroll window and selection bounds.

use vita_syncthing_app::domain::{FolderPicker, PickerItem, PickerMode, UiText, BROWSER_ROWS};

fn picker(len: usize) -> FolderPicker {
    let mut p = FolderPicker::new("ux0:");
    p.items = (0..len)
        .map(|i| PickerItem {
            label: UiText::raw(format!("dir {i}")),
            path: format!("ux0:dir{i}"),
            is_preset: false,
            template: None,
            is_shared: false,
        })
        .collect();
    p
}

#[test]
fn test_new_browses_the_root_in_change_target_mode() {
    let p = FolderPicker::new("ux0:data");
    assert_eq!(p.mode, PickerMode::ChangeShareTarget);
    assert_eq!(p.current_dir, "ux0:data");
    assert!(p.items.is_empty());
    assert_eq!((p.selected_idx, p.top_idx), (0, 0));
}

#[test]
fn test_browser_shows_eight_rows() {
    assert_eq!(BROWSER_ROWS, 8);
}

#[test]
fn test_clamp_scroll_on_an_empty_list_resets_the_window() {
    let mut p = picker(0);
    p.selected_idx = 3;
    p.top_idx = 2;
    p.clamp_scroll();
    assert_eq!((p.selected_idx, p.top_idx), (0, 0));
}

#[test]
fn test_clamp_scroll_pulls_a_selection_past_the_end_back_into_the_list() {
    let mut p = picker(5);
    p.selected_idx = 9;
    p.clamp_scroll();
    assert_eq!(p.selected_idx, 4);
    assert_eq!(p.top_idx, 0);
}

#[test]
fn test_clamp_scroll_moves_the_window_up_to_a_selection_above_it() {
    let mut p = picker(20);
    p.top_idx = 7;
    p.selected_idx = 4;
    p.clamp_scroll();
    assert_eq!(p.top_idx, 4);
    assert_eq!(p.selected_idx, 4);
}

#[test]
fn test_clamp_scroll_keeps_the_window_when_the_selection_is_visible() {
    let mut p = picker(20);
    p.top_idx = 3;
    // Last visible row of the window: top + rows - 1.
    p.selected_idx = 3 + BROWSER_ROWS - 1;
    p.clamp_scroll();
    assert_eq!(p.top_idx, 3);
}

#[test]
fn test_clamp_scroll_moves_the_window_down_to_a_selection_below_it() {
    let mut p = picker(20);
    p.selected_idx = BROWSER_ROWS; // first row not visible from top 0
    p.clamp_scroll();
    assert_eq!(p.top_idx, 1);
    assert_eq!(p.selected_idx, BROWSER_ROWS);

    p.selected_idx = 19;
    p.clamp_scroll();
    assert_eq!(p.top_idx, 19 + 1 - BROWSER_ROWS);
}

#[test]
fn test_scroll_rows_moves_the_window_and_keeps_selection_visible() {
    let mut p = picker(20);
    p.scroll_rows(5);
    assert_eq!(p.top_idx, 5);
    // Selection (0) was above the window: it is moved to the first visible row.
    assert_eq!(p.selected_idx, 5);
}

#[test]
fn test_scroll_rows_is_bounded_at_both_ends() {
    let mut p = picker(20);
    p.scroll_rows(1000);
    assert_eq!(p.top_idx, 20 - BROWSER_ROWS, "last full window");
    assert_eq!(
        p.selected_idx,
        20 - BROWSER_ROWS,
        "selection moved to the first visible row"
    );

    p.scroll_rows(-1000);
    assert_eq!(p.top_idx, 0);
    assert_eq!(p.selected_idx, BROWSER_ROWS - 1);

    p.scroll_rows(-1);
    assert_eq!(p.top_idx, 0, "already at the top");
}

#[test]
fn test_scroll_rows_on_a_short_list_does_not_scroll() {
    let mut p = picker(5);
    p.selected_idx = 2;
    p.scroll_rows(3);
    assert_eq!(p.top_idx, 0);
    assert_eq!(p.selected_idx, 2);
    p.scroll_rows(-3);
    assert_eq!((p.top_idx, p.selected_idx), (0, 2));
}

#[test]
fn test_scroll_rows_on_an_empty_list_is_a_no_op() {
    let mut p = picker(0);
    p.scroll_rows(4);
    p.scroll_rows(-4);
    assert_eq!((p.top_idx, p.selected_idx), (0, 0));
}

#[test]
fn test_scroll_rows_clamps_the_selection_inside_the_visible_rows() {
    let mut p = picker(20);
    p.selected_idx = 10;
    p.top_idx = 10;
    // Scroll back: window 6..14, selection 10 stays visible.
    p.scroll_rows(-4);
    assert_eq!(p.top_idx, 6);
    assert_eq!(p.selected_idx, 10);
    // Scroll far: window 0..8, selection pulled to the last visible row 7.
    p.scroll_rows(-100);
    assert_eq!((p.top_idx, p.selected_idx), (0, 7));
}
