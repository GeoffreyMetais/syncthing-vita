//! `Shares` page: touch, `Cross` on the focused element and face-button shortcuts are all
//! resolved from the elements the screen declares while drawing (`HitMap`).

mod common;

use common::{draw, press, rect_of, tap, tap_rect, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{InputController, ShareService};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, FocusSlot, Language, LanguagePreference, ModalDialog, PickerMode, UiAction,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_RIGHT, BTN_SQUARE,
    BTN_TRIANGLE,
};

const PEER: DeviceId = DeviceId([0x42; 32]);

/// State on the `Shares` tab with `count` shares linked to `PEER`.
fn setup(name: &str, count: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    for i in 0..count {
        let id = format!("share-{i}");
        ShareService::create_share(&mut state, &repo, &id, &id, &format!("{dir}/{id}"));
        let share_idx = state.shares.len() - 1;
        ShareService::toggle_device_for_share(&mut state, &repo, share_idx, PEER);
    }
    state.modal = ModalDialog::None;
    state.active_tab = ActiveTab::Shares;
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
    // Sidebar -> page, then down to the second share.
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(state.selected_share_idx, 1);

    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 1 }
    );
    state.modal = ModalDialog::None;

    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;

    press(&mut state, &repo, BTN_TRIANGLE);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(state.picker_mode, PickerMode::ChangeShareTarget);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_activates_the_focused_element() {
    let (dir, repo, mut state) = setup("shares-cross-focus", 1);
    press(&mut state, &repo, BTN_RIGHT);
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
