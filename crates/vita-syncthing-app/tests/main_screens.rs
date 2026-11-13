//! Main screens (sidebar, hint bar, `Overview`, `Files`, `Settings`): input is resolved from
//! the elements each screen declares while drawing, in every language.

mod common;

use common::{draw, press, rect_of, tap, tap_action, tap_rect, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{ShareService, UserCommand};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, Language, LanguagePreference, ModalDialog, PickerMode, Rect, UiAction,
    TAB_ORDER,
};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_SQUARE, BTN_TRIANGLE};
use vita_syncthing_app::ui::gfx::{GFX_H, GFX_W};
use vita_syncthing_app::ui::theme::HINT_Y;

const PEER: DeviceId = DeviceId([0x42; 32]);
const LANGUAGES: [Language; 5] = [
    Language::English,
    Language::French,
    Language::Spanish,
    Language::German,
    Language::Italian,
];

/// State with one share linked to `PEER` and a few folders to browse.
fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    for sub in ["alpha", "beta", "gamma"] {
        std::fs::create_dir_all(format!("{dir}/{sub}/inner")).unwrap();
    }
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/gamma"));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER);
    state.modal = ModalDialog::None;
    (dir, repo, state)
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[test]
fn test_every_declared_element_is_on_screen_and_hints_do_not_overlap() {
    let (dir, repo, mut state) = setup("screens-geometry");
    for language in LANGUAGES {
        state.set_language_preference(LanguagePreference::Fixed(language));
        for tab in TAB_ORDER {
            state.modal = ModalDialog::None;
            tap_action(&mut state, &repo, UiAction::SwitchTab(tab));
            assert_eq!(state.active_tab, tab);
            draw(&mut state);
            let hotspots = state.hit_map.hotspots().to_vec();
            for h in &hotspots {
                let r = h.rect;
                assert!(
                    r.x >= 0 && r.y >= 0 && r.x + r.w <= GFX_W && r.y + r.h <= GFX_H,
                    "{:?} is off screen on {tab:?} in {language:?}: {r:?}",
                    h.action
                );
            }
            let hints: Vec<&Rect> = hotspots
                .iter()
                .map(|h| &h.rect)
                .filter(|r| r.y >= HINT_Y)
                .collect();
            for (i, a) in hints.iter().enumerate() {
                for b in &hints[i + 1..] {
                    assert!(
                        !overlap(a, b),
                        "hints overlap on {tab:?} in {language:?}: {a:?} / {b:?}"
                    );
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_sidebar_tabs_and_transfer_card() {
    let (dir, repo, mut state) = setup("screens-sidebar");
    tap_action(
        &mut state,
        &repo,
        UiAction::SwitchTab(ActiveTab::FolderPicker),
    );
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    // The `Files` tab opens on the share-creation browser.
    assert_eq!(state.picker_mode, PickerMode::CreateNewShare);

    tap_action(&mut state, &repo, UiAction::SwitchTab(ActiveTab::Dashboard));
    // The sidebar transfer card is the first `StartSync` element, the button the last.
    let card = rect_of(&mut state, UiAction::StartSync);
    assert_eq!(tap_rect(&mut state, &repo, card), UserCommand::StartSync);
    assert_eq!(
        tap_action(&mut state, &repo, UiAction::StartSync),
        UserCommand::StartSync
    );
    // No second sync is started while one is running.
    state.sync_in_progress = true;
    assert_eq!(
        tap_action(&mut state, &repo, UiAction::StartSync),
        UserCommand::None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_overview_card_buttons_and_hints() {
    let (dir, repo, mut state) = setup("screens-overview");
    // Touching the device card shows the ID.
    let card = rect_of(&mut state, UiAction::ShowDeviceId);
    tap(
        &mut state,
        &repo,
        (card.x + 40) as usize,
        (card.y + 40) as usize,
    );
    assert_eq!(state.modal, ModalDialog::ShowDeviceId);
    state.modal = ModalDialog::None;

    // The share list is on the `Overview`: `Square` asks to delete the selected share.
    let share_card = rect_of(&mut state, UiAction::SelectShare(0));
    assert!(share_card.y >= card.y + card.h);
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(state.modal, ModalDialog::ConfirmDeleteShare { share_idx: 0 });
    state.modal = ModalDialog::None;

    // `Triangle` and its hint both open the share creation dialog.
    press(&mut state, &repo, BTN_TRIANGLE);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;
    tap_action(&mut state, &repo, UiAction::NewShare);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;

    tap_action(&mut state, &repo, UiAction::StartScreensaver);
    assert!(state.oled_saver_active);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_files_row_selects_and_open_chip_enters_the_folder() {
    let (dir, repo, mut state) = setup("screens-files");
    tap_action(
        &mut state,
        &repo,
        UiAction::SwitchTab(ActiveTab::FolderPicker),
    );
    let alpha = state
        .picker_items
        .iter()
        .position(|item| item.path.ends_with("/alpha"))
        .expect("alpha is listed");

    // The `Open` chip enters the folder...
    tap_action(&mut state, &repo, UiAction::PickerOpen(alpha));
    assert!(state.picker_current_dir.ends_with("/alpha"));
    // ...the `Up` button and its hint go back...
    tap_action(&mut state, &repo, UiAction::PickerUp);
    assert_eq!(state.picker_current_dir, dir);

    // ...and the rest of the row chooses the folder (here: starts sharing it).
    let row = rect_of(&mut state, UiAction::PickerSelect(alpha));
    tap(
        &mut state,
        &repo,
        (row.x + 60) as usize,
        (row.y + row.h / 2) as usize,
    );
    assert!(state
        .shares
        .iter()
        .any(|share| share.target_path.ends_with("/alpha")));

    // A folder that is already shared declares no `Open` chip.
    state.modal = ModalDialog::None;
    tap_action(
        &mut state,
        &repo,
        UiAction::SwitchTab(ActiveTab::FolderPicker),
    );
    let gamma = state
        .picker_items
        .iter()
        .position(|item| item.path.ends_with("/gamma"))
        .expect("gamma is listed");
    draw(&mut state);
    assert!(state.picker_items[gamma].is_shared);
    assert!(!state
        .hit_map
        .hotspots()
        .iter()
        .any(|h| h.action == UiAction::PickerOpen(gamma)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_settings_hints_trigger_their_actions() {
    let (dir, repo, mut state) = setup("screens-settings");
    tap_action(&mut state, &repo, UiAction::SwitchTab(ActiveTab::Settings));
    tap_action(&mut state, &repo, UiAction::ConfirmQuit);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);
    state.modal = ModalDialog::None;

    assert_eq!(
        tap_action(&mut state, &repo, UiAction::AddDevice),
        UserCommand::OpenSystemKeyboard
    );
    assert!(matches!(state.modal, ModalDialog::AddDeviceById { .. }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_settings_devices_card_is_separate_from_theme_card() {
    use vita_syncthing_app::domain::{NavFocus, SETTINGS_ROW_DEVICES};
    use vita_syncthing_app::infrastructure::{BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT};

    let (dir, repo, mut state) = setup("screens-settings-devices");
    tap_action(&mut state, &repo, UiAction::SwitchTab(ActiveTab::Settings));
    draw(&mut state);

    // The Display & Theme card hotspot must not contain the AddDevice or ShowDeviceId buttons.
    let theme_card = state
        .hit_map
        .hotspots()
        .iter()
        .find(|h| h.action == UiAction::ToggleTheme && h.focus.is_none())
        .expect("theme card hotspot")
        .rect;
    let add_btn = state
        .hit_map
        .hotspots()
        .iter()
        .find(|h| h.action == UiAction::AddDevice && h.focus.is_some())
        .expect("add device button")
        .rect;
    let qr_btn = state
        .hit_map
        .hotspots()
        .iter()
        .find(|h| h.action == UiAction::ShowDeviceId && h.focus.is_some())
        .expect("qr/id button")
        .rect;
    assert!(
        !overlap(&theme_card, &add_btn),
        "Add device button should be outside the Colour Theme card"
    );
    assert!(
        !overlap(&theme_card, &qr_btn),
        "QR / ID button should be outside the Colour Theme card"
    );

    // D-pad Down from Display reaches the Devices card; col 0 is QR / ID, Right moves to Add device.
    state.nav.focus = NavFocus::Content;
    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_DEVICES));
    assert_eq!(state.focused_action(ActiveTab::Settings), Some(0));

    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::ShowDeviceId);
    state.modal = ModalDialog::None;

    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.focused_action(ActiveTab::Settings), Some(1));
    assert_eq!(
        press(&mut state, &repo, BTN_CROSS),
        UserCommand::OpenSystemKeyboard
    );
    assert!(matches!(state.modal, ModalDialog::AddDeviceById { .. }));
    state.modal = ModalDialog::None;

    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.focused_action(ActiveTab::Settings), Some(0));

    let _ = std::fs::remove_dir_all(&dir);
}
