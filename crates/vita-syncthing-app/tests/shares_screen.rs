//! Share list of the `Overview` page: touch, `Cross` on the focused element and face-button shortcuts are all
//! resolved from the elements the screen declares while drawing (`HitMap`).

mod common;

use common::{drag_over, draw, press, rect_of, tap, tap_rect, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, InputController, ShareService};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, FocusSlot, Language, LanguagePreference, ModalDialog, NavFocus,
    ScrollTarget, UiAction, VISIBLE_SHARES,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_RIGHT, BTN_SQUARE,
    BTN_TRIANGLE, BTN_UP,
};

const PEER: DeviceId = DeviceId([0x42; 32]);

/// State on the `Overview` tab with `count` shares linked to `PEER`.
fn setup(name: &str, count: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    DeviceRegistry::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    for i in 0..count {
        let id = format!("share-{i}");
        ShareService::create_share(&mut state, &repo, &id, &id, &format!("{dir}/{id}"));
        let share_idx = state.shares.len() - 1;
        ShareService::toggle_device_for_share(&mut state, &repo, share_idx, PEER);
    }
    state.modal = ModalDialog::None;
    state.active_tab = ActiveTab::Dashboard;
    (dir, repo, state)
}

#[test]
fn test_touch_targets_follow_the_drawn_buttons_in_every_language() {
    let (dir, repo, mut state) = setup("shares-touch-languages", 2);
    let mut delete_positions = Vec::new();
    for pref in [
        LanguagePreference::Fixed(Language::English),
        LanguagePreference::Fixed(Language::French),
        LanguagePreference::Fixed(Language::Spanish),
        LanguagePreference::Fixed(Language::German),
        LanguagePreference::Fixed(Language::Italian),
    ] {
        state.set_language_preference(pref);
        draw(&mut state);
        let delete = rect_of(&mut state, UiAction::DeleteShare(1));
        delete_positions.push(delete.x);
        tap_rect(&mut state, &repo, delete);
        assert_eq!(
            state.modal,
            ModalDialog::ConfirmDeleteShare { share_idx: 1 }
        );
        assert_eq!(state.selected_share_idx, 1);
        state.modal = ModalDialog::None;

        draw(&mut state);
        let devices = rect_of(&mut state, UiAction::ManageDevices(0));
        tap_rect(&mut state, &repo, devices);
        assert_eq!(
            state.modal,
            ModalDialog::ManageDevices {
                share_idx: 0,
                selected_dev_idx: 0
            }
        );
        state.modal = ModalDialog::None;
    }
    // Label widths differ between languages, so the buttons do move: fixed touch
    // coordinates could not have matched all of them.
    delete_positions.dedup();
    assert!(delete_positions.len() > 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_tapping_a_card_selects_it_and_new_share_opens_the_dialog() {
    let (dir, repo, mut state) = setup("shares-touch-card", 2);
    draw(&mut state);
    let card = rect_of(&mut state, UiAction::SelectShare(1));
    // Left part of the card: away from the row's buttons.
    tap(
        &mut state,
        &repo,
        (card.x + 40) as usize,
        (card.y + 20) as usize,
    );
    assert_eq!(state.selected_share_idx, 1);
    assert_eq!(state.modal, ModalDialog::None);

    draw(&mut state);
    let new_share = rect_of(&mut state, UiAction::NewShare);
    tap_rect(&mut state, &repo, new_share);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_face_buttons_trigger_the_element_that_displays_them() {
    let (dir, repo, mut state) = setup("shares-shortcuts", 2);
    // Sidebar -> device card, then down to the first share and the second one.
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_DOWN);
    state.selected_share_idx = 0;
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 1);

    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 1 }
    );
    state.modal = ModalDialog::None;

    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::ShowDeviceId);
    state.modal = ModalDialog::None;

    // A share's folder cannot be changed: on a share row, `Triangle` is the page's `New Share`.
    press(&mut state, &repo, BTN_TRIANGLE);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    assert_eq!(state.active_tab, ActiveTab::Dashboard);
    assert_eq!(state.selected_share_idx, 1);
    state.modal = ModalDialog::None;

    // Each share row declares only its `Devices` and `Delete` buttons.
    draw(&mut state);
    let row_actions: Vec<UiAction> = state
        .hit_map
        .hotspots()
        .iter()
        .filter(|h| matches!(h.focus, Some(FocusSlot::RowAction { row: 1, .. })))
        .map(|h| h.action)
        .collect();
    assert_eq!(
        row_actions,
        vec![UiAction::ManageDevices(1), UiAction::DeleteShare(1)]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_activates_the_focused_element() {
    let (dir, repo, mut state) = setup("shares-cross-focus", 1);
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_DOWN);
    // The focus ring is on `Devices`; the frame declares that slot.
    draw(&mut state);
    let focused = FocusSlot::RowAction { row: 0, col: 0 };
    assert_eq!(
        state.hit_map.focused_action(focused),
        Some(UiAction::ManageDevices(0))
    );
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_element_of_a_share_removed_since_the_frame_is_ignored() {
    let (dir, repo, mut state) = setup("shares-stale-frame", 1);
    draw(&mut state);
    let delete = rect_of(&mut state, UiAction::DeleteShare(0));
    // The share disappears (e.g. sync event) before the next frame is drawn.
    state.shares.clear();
    InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_tap: Some(((delete.x + 4) as usize, (delete.y + 4) as usize)),
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_no_page_element_is_declared_behind_the_screensaver() {
    let (dir, _repo, mut state) = setup("shares-saver", 1);
    draw(&mut state);
    assert!(!state.hit_map.hotspots().is_empty());
    state.oled_saver_active = true;
    draw(&mut state);
    assert!(state.hit_map.hotspots().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Indexes of the share cards drawn in the current frame.
fn drawn_shares(state: &mut AppState) -> Vec<usize> {
    draw(state);
    state
        .hit_map
        .hotspots()
        .iter()
        .filter_map(|h| match h.action {
            UiAction::SelectShare(i) => Some(i),
            _ => None,
        })
        .collect()
}

#[test]
fn test_dragging_over_the_share_list_scrolls_it_and_keeps_the_selection_visible() {
    let (dir, repo, mut state) = setup("shares-drag", 9);
    state.selected_share_idx = 0;
    draw(&mut state);
    let step = state
        .hit_map
        .scroll_areas()
        .iter()
        .find(|a| a.target == ScrollTarget::ShareList)
        .expect("share list scroll area")
        .step;
    assert_eq!(drawn_shares(&mut state), vec![0, 1, 2, 3]);

    // Finger moved up by two cards: the list follows it, the selection stays on screen.
    drag_over(&mut state, &repo, ScrollTarget::ShareList, -2 * step, 6);
    assert_eq!(drawn_shares(&mut state), vec![2, 3, 4, 5]);
    assert_eq!(state.selected_share_idx, 2);
    // A drag opens nothing.
    assert_eq!(state.modal, ModalDialog::None);

    // Dragging far past the end stops on the last full window.
    drag_over(&mut state, &repo, ScrollTarget::ShareList, -20 * step, 10);
    assert_eq!(drawn_shares(&mut state), vec![5, 6, 7, 8]);
    assert_eq!(state.selected_share_idx, 5);

    // Finger moved down by one card: back one share.
    drag_over(&mut state, &repo, ScrollTarget::ShareList, step, 4);
    assert_eq!(drawn_shares(&mut state), vec![4, 5, 6, 7]);
    // And far past the start: the first window.
    drag_over(&mut state, &repo, ScrollTarget::ShareList, 20 * step, 10);
    assert_eq!(drawn_shares(&mut state), vec![0, 1, 2, 3]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_dpad_moves_scroll_from_the_window_on_screen() {
    let (dir, repo, mut state) = setup("shares-drag-dpad", 9);
    state.selected_share_idx = 0;
    state.nav.focus = NavFocus::Content;
    state.nav.header = false;
    draw(&mut state);
    let step = state.hit_map.scroll_areas()[0].step;
    drag_over(&mut state, &repo, ScrollTarget::ShareList, -3 * step, 3);
    assert_eq!(drawn_shares(&mut state), vec![3, 4, 5, 6]);
    assert_eq!(state.selected_share_idx, 3);

    // Down within the window keeps it; past its last card it scrolls by one.
    for _ in 0..3 {
        press(&mut state, &repo, BTN_DOWN);
    }
    assert_eq!(state.selected_share_idx, 6);
    assert_eq!(drawn_shares(&mut state), vec![3, 4, 5, 6]);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(drawn_shares(&mut state), vec![4, 5, 6, 7]);

    // Up within the window keeps it too (no jump back to the selection at the bottom).
    press(&mut state, &repo, BTN_UP);
    press(&mut state, &repo, BTN_UP);
    assert_eq!(state.selected_share_idx, 5);
    assert_eq!(drawn_shares(&mut state), vec![4, 5, 6, 7]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_short_share_list_does_not_scroll() {
    let (dir, repo, mut state) = setup("shares-drag-short", VISIBLE_SHARES - 1);
    state.selected_share_idx = 0;
    drag_over(&mut state, &repo, ScrollTarget::ShareList, -300, 5);
    assert_eq!(drawn_shares(&mut state), vec![0, 1, 2]);
    assert_eq!(state.selected_share_idx, 0);
    let _ = std::fs::remove_dir_all(&dir);
}
