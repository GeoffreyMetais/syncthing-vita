//! `Settings` page controller: D-pad focus over the cards (display, devices, language, declined
//! folders, log) and their buttons, and the persisted choices (theme, language, declined folders).

mod common;

use common::{press, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::domain::{
    ActiveTab, AppState, IgnoredFolder, Language, LanguagePreference, ModalDialog, NavFocus,
    ThemeMode, SETTINGS_ROW_DECLINED, SETTINGS_ROW_DEVICES, SETTINGS_ROW_DISPLAY,
    SETTINGS_ROW_LANGUAGE, SETTINGS_ROW_LOG,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP,
};

/// State on the `Settings` page with its content focused.
fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.active_tab = ActiveTab::Settings;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    (dir, repo, state)
}

#[test]
fn test_down_walks_the_cards_and_stops_at_the_log_card() {
    let (dir, repo, mut state) = setup("settings-down");
    assert!(state.is_settings_row_focused(SETTINGS_ROW_DISPLAY));
    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_DEVICES));
    for _ in 0..10 {
        press(&mut state, &repo, BTN_DOWN);
    }
    assert!(state.is_settings_row_focused(SETTINGS_ROW_LOG));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_up_walks_back_and_stops_at_the_display_card() {
    let (dir, repo, mut state) = setup("settings-up");
    for _ in 0..SETTINGS_ROW_LANGUAGE {
        press(&mut state, &repo, BTN_DOWN);
    }
    assert!(state.is_settings_row_focused(SETTINGS_ROW_LANGUAGE));
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_DEVICES));
    for _ in 0..10 {
        press(&mut state, &repo, BTN_UP);
    }
    assert!(state.is_settings_row_focused(SETTINGS_ROW_DISPLAY));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_moving_to_another_card_resets_the_button_column() {
    let (dir, repo, mut state) = setup("settings-col-reset");
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 1);
    // Up to the display card (one button): its column is the first one.
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_DISPLAY));
    assert_eq!(state.nav.col, 0);
    // Down to the devices card: Down always enters the first button.
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_LANGUAGE));
    assert_eq!(state.nav.col, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_devices_card_buttons_move_left_and_right_then_leave_to_sidebar() {
    let (dir, repo, mut state) = setup("settings-devices-row");
    press(&mut state, &repo, BTN_DOWN);
    // QR / ID (col 0) then + Add Device (col 1); Right stops on the last button.
    press(&mut state, &repo, BTN_RIGHT);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 1);
    press(&mut state, &repo, BTN_CROSS);
    assert!(matches!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: None,
            ..
        }
    ));
    state.modal = ModalDialog::None;
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.col, 0);
    assert_eq!(state.nav.focus, NavFocus::Content);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_single_button_card_left_returns_to_sidebar() {
    let (dir, repo, mut state) = setup("settings-left-single");
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.focus, NavFocus::Content);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_on_display_card_toggles_and_persists_the_theme() {
    let (dir, repo, mut state) = setup("settings-theme-cross");
    assert_eq!(state.theme_mode, ThemeMode::Oled);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.theme_mode, ThemeMode::Dark);
    assert_eq!(repo.load_theme(), ThemeMode::Dark);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_on_language_card_cycles_and_persists_the_language() {
    let (dir, repo, mut state) = setup("settings-lang-cross");
    state.set_system_language(Language::Spanish);
    for _ in 0..SETTINGS_ROW_LANGUAGE {
        press(&mut state, &repo, BTN_DOWN);
    }
    // System (Español) -> English
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.language, Language::English);
    assert_eq!(
        repo.load_language(),
        LanguagePreference::Fixed(Language::English)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_on_declined_card_forgets_declined_folders_and_saves_it() {
    let (dir, repo, mut state) = setup("settings-declined-cross");
    let peer = DeviceId([0x42; 32]);
    state.ignored_folders.push(IgnoredFolder {
        folder_id: "docs".to_string(),
        device_id: peer,
    });
    repo.save_ignored_folders(&state.ignored_folders);
    for _ in 0..SETTINGS_ROW_DECLINED {
        press(&mut state, &repo, BTN_DOWN);
    }
    press(&mut state, &repo, BTN_CROSS);
    assert!(state.ignored_folders.is_empty());
    assert!(repo.load_ignored_folders().is_empty());
    assert!(state.config_dirty, "the offers are reconsidered by the worker");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_on_log_card_opens_the_log_viewer() {
    let (dir, repo, mut state) = setup("settings-log-cross");
    for _ in 0..SETTINGS_ROW_LOG {
        press(&mut state, &repo, BTN_DOWN);
    }
    press(&mut state, &repo, BTN_CROSS);
    assert!(state.log_view.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_theme_and_language_choices_do_not_touch_each_other() {
    let (dir, repo, mut state) = setup("settings-independent");
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.language_pref, LanguagePreference::System);
    assert_eq!(repo.load_language(), LanguagePreference::System);
    let _ = std::fs::remove_dir_all(&dir);
}
