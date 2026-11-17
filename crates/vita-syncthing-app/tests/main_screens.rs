//! Main screens (sidebar, hint bar, `Overview`, `Files`, `Settings`): input is resolved from
//! the elements each screen declares while drawing, in every language.

mod common;

use common::{draw, press, rect_of, tap, tap_action, tap_rect, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, ShareService, UserCommand};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, Language, LanguagePreference, ModalDialog, PickerMode, PluginInfo,
    PluginRequest, Rect, UiAction, UiText, TAB_ORDER,
};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_SQUARE, BTN_START, BTN_TRIANGLE};
use vita_syncthing_app::ui::gfx::{FontWeight, Gfx, PadButton, GFX_H, GFX_W};
use vita_syncthing_app::ui::modals::confirm_frame::CONFIRM_PAD_X;
use vita_syncthing_app::ui::renderer::draw_frame;
use vita_syncthing_app::ui::screens::settings_plugin::{
    plugin_card_layout, plugin_description, PLUGIN_DESC_SIZE,
};
use vita_syncthing_app::ui::strings;
use vita_syncthing_app::ui::theme::{
    UiTheme, CONFIRM_PLUGIN_W, CONTENT_R, CONTENT_X, HINT_Y, SETTINGS_PLUGIN_B, SETTINGS_PLUGIN_Y,
    SIDEBAR_W,
};
use vita_syncthing_app::ui::widgets::action_width;

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
    let repo = ConfigRepository::new(&format!("{dir}/.appdata"));
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    DeviceRegistry::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
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
    assert_eq!(state.picker.mode, PickerMode::CreateNewShare);

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

/// Draws the frame and tells whether anything is drawn where the transfer card's bar goes.
fn transfer_bar_drawn(state: &mut AppState) -> bool {
    let card = rect_of(state, UiAction::StartSync);
    let mut gfx = Gfx::new();
    draw_frame(&mut gfx, state);
    let card_bg = UiTheme::for_mode(state.theme_mode).card;
    (card.y + 60..card.y + 66)
        .any(|y| (24..SIDEBAR_W - 64).any(|x| gfx.pixels[(y * GFX_W + x) as usize] != card_bg))
}

#[test]
fn test_transfer_card_progress_bar_only_while_transferring() {
    let (dir, _repo, mut state) = setup("screens-transfer-bar");
    // Idle (no connected session) and Ready (connected, up to date): no bar.
    assert!(!transfer_bar_drawn(&mut state));
    state.session_connected = true;
    assert!(!transfer_bar_drawn(&mut state));
    // Uploading: indeterminate bar.
    state.uploading = true;
    assert!(transfer_bar_drawn(&mut state));
    // Downloading: determinate bar, even at 0 %.
    state.uploading = false;
    state.sync_in_progress = true;
    state.progress_pct = Some(0);
    assert!(transfer_bar_drawn(&mut state));
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
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 }
    );
    state.modal = ModalDialog::None;

    // `Triangle` and its hint both open the share creation dialog.
    press(&mut state, &repo, BTN_TRIANGLE);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;
    tap_action(&mut state, &repo, UiAction::NewShare);
    assert_eq!(state.modal, ModalDialog::CreateShare { selected_idx: 0 });
    state.modal = ModalDialog::None;

    // `START` and its hint both ask for confirmation before quitting.
    press(&mut state, &repo, BTN_START);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);
    state.modal = ModalDialog::None;
    tap_action(&mut state, &repo, UiAction::ConfirmQuit);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);
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
        .picker
        .items
        .iter()
        .position(|item| item.path.ends_with("/alpha"))
        .expect("alpha is listed");

    // The `Open` chip enters the folder...
    tap_action(&mut state, &repo, UiAction::PickerOpen(alpha));
    assert!(state.picker.current_dir.ends_with("/alpha"));
    // ...the `Up` button and its hint go back...
    tap_action(&mut state, &repo, UiAction::PickerUp);
    assert_eq!(state.picker.current_dir, dir);

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
        .picker
        .items
        .iter()
        .position(|item| item.path.ends_with("/gamma"))
        .expect("gamma is listed");
    draw(&mut state);
    assert!(state.picker.items[gamma].is_shared);
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

/// Every state of the background sync plugin card (the last one: uninstalled, still loaded).
fn plugin_states() -> [PluginInfo; 5] {
    [
        PluginInfo::new(false, false, true),
        PluginInfo::new(true, false, true),
        PluginInfo::new(true, true, true),
        PluginInfo::new(true, true, false),
        PluginInfo::new(false, true, false),
    ]
}

#[test]
fn test_settings_plugin_card_fits_in_every_language_and_state() {
    let (dir, repo, mut state) = setup("screens-plugin-card");
    tap_action(&mut state, &repo, UiAction::SwitchTab(ActiveTab::Settings));
    let mut gfx = Gfx::new();
    let card = Rect::new(
        CONTENT_X,
        SETTINGS_PLUGIN_Y,
        CONTENT_R - CONTENT_X,
        SETTINGS_PLUGIN_B - SETTINGS_PLUGIN_Y,
    );
    let plugin_actions = [
        UiAction::InstallPlugin,
        UiAction::UninstallPlugin,
        UiAction::TogglePluginEnabled,
    ];
    for language in LANGUAGES {
        state.set_language_preference(LanguagePreference::Fixed(language));
        for info in plugin_states() {
            state.plugin = info;
            draw(&mut state);
            let hotspots = state.hit_map.hotspots().to_vec();
            let buttons: Vec<Rect> = hotspots
                .iter()
                .filter(|h| plugin_actions.contains(&h.action))
                .map(|h| h.rect)
                .collect();
            assert_eq!(buttons.len(), info.action_count());
            for b in &buttons {
                assert!(
                    b.x >= card.x && b.x + b.w <= card.x + card.w,
                    "{b:?} outside the card ({language:?}, {info:?})"
                );
                assert!(b.y >= card.y - 4 && b.y + b.h <= card.y + card.h);
            }
            // Focusable buttons of the whole page never cover each other.
            let focusable: Vec<Rect> = hotspots
                .iter()
                .filter(|h| h.focus.is_some())
                .map(|h| h.rect)
                .collect();
            for (i, a) in focusable.iter().enumerate() {
                for b in &focusable[i + 1..] {
                    assert!(!overlap(a, b), "{a:?} / {b:?} ({language:?}, {info:?})");
                }
            }
            // No text of the card is cut.
            let layout = plugin_card_layout(&mut gfx, &state);
            let p = strings::strings(language).plugin;
            assert!(
                gfx.text_width(12, FontWeight::Bold, p.section) <= layout.title_max_w,
                "section title cut in {language:?} ({info:?})"
            );
            let desc = plugin_description(&state);
            assert!(
                gfx.text_width(PLUGIN_DESC_SIZE, FontWeight::Regular, desc) <= layout.desc_max_w,
                "description cut in {language:?} ({info:?}): {desc}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_confirm_plugin_texts_fit_in_every_language() {
    let mut gfx = Gfx::new();
    let inner = CONFIRM_PLUGIN_W - 2 * CONFIRM_PAD_X;
    for language in LANGUAGES {
        let p = strings::strings(language).plugin;
        for request in [PluginRequest::Install, PluginRequest::Uninstall] {
            let t = p.confirm(request);
            let checks = [
                (21, FontWeight::Bold, t.title, inner - 52),
                (14, FontWeight::Regular, t.subtitle, inner - 52),
                (14, FontWeight::Regular, t.lines[0], inner),
                (14, FontWeight::Regular, t.lines[1], inner),
                (14, FontWeight::Medium, t.reboot, inner),
            ];
            for (size, weight, text, max) in checks {
                assert!(
                    gfx.text_width(size, weight, text) <= max,
                    "{text:?} is cut in {language:?} ({request:?})"
                );
            }
        }
    }
}

#[test]
fn test_background_worker_wait_texts_fit_in_every_language() {
    // Same room as the `Overview` device card status line (left of its `QR / ID` and
    // `New Share` buttons) and the sidebar transfer card label.
    let mut gfx = Gfx::new();
    for language in LANGUAGES {
        let s = strings::strings(language);
        let new_w = action_width(&mut gfx, PadButton::Triangle, s.action_new_share);
        let qr_w = action_width(&mut gfx, PadButton::Circle, s.action_qr_id);
        let status_room = (CONTENT_R - 16 - new_w - 10 - qr_w) - 16 - (CONTENT_X + 20 + 82);
        let status = s.text(&UiText::WaitingForBackgroundSync);
        assert!(
            gfx.text_width(15, FontWeight::Medium, &status) <= status_room,
            "{status:?} is cut in {language:?}"
        );
        let label = s.text(&UiText::BackgroundSyncStopping);
        assert!(
            gfx.text_width(14, FontWeight::Medium, &label) <= SIDEBAR_W - 48,
            "{label:?} is cut in {language:?}"
        );
    }
}
