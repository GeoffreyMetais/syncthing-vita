//! UI language setting: follows the PS Vita system language by default, can be fixed from
//! `Settings` (D-pad or touch) and is persisted in `language.cfg`.

mod common;

use common::temp_test_dir;
use vita_syncthing_app::application::{InputController, UserCommand};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, Language, LanguagePreference, NavFocus, SETTINGS_ROW_LANGUAGE,
};
use vita_syncthing_app::infrastructure::{ConfigRepository, InputFrame, BTN_CROSS, BTN_DOWN};
use vita_syncthing_app::ui::theme::{SETTINGS_LANGUAGE_B, SETTINGS_LANGUAGE_Y};

fn press(state: &mut AppState, repo: &ConfigRepository, buttons: u32) -> UserCommand {
    InputController::handle_input(
        state,
        repo,
        InputFrame {
            pressed: buttons,
            any_activity: true,
            ..Default::default()
        },
    )
}

fn settings_state(dir: &str) -> AppState {
    let mut state = AppState::new(Vec::new(), Vec::new(), dir);
    state.active_tab = ActiveTab::Settings;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    state
}

#[test]
fn test_language_preference_codec_and_cycle() {
    assert_eq!(LanguagePreference::System.as_str(), "system");
    assert_eq!(LanguagePreference::Fixed(Language::German).as_str(), "de");
    assert_eq!(
        LanguagePreference::parse(" SYSTEM\n"),
        Some(LanguagePreference::System)
    );
    assert_eq!(
        LanguagePreference::parse("fr\n"),
        Some(LanguagePreference::Fixed(Language::French))
    );
    assert_eq!(LanguagePreference::parse("klingon"), None);

    // System -> en -> fr -> es -> de -> it -> System
    let mut pref = LanguagePreference::System;
    let mut seen = Vec::new();
    for _ in 0..6 {
        pref = pref.next();
        seen.push(pref.as_str());
    }
    assert_eq!(seen, ["en", "fr", "es", "de", "it", "system"]);

    assert_eq!(
        LanguagePreference::System.resolve(Language::Italian),
        Language::Italian
    );
    assert_eq!(
        LanguagePreference::Fixed(Language::Spanish).resolve(Language::Italian),
        Language::Spanish
    );
}

#[test]
fn test_language_defaults_to_system_and_persists() {
    let dir = temp_test_dir("language-persist");
    let repo = ConfigRepository::new(&dir);

    // No language.cfg: follow the system language
    assert_eq!(repo.load_language(), LanguagePreference::System);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.set_system_language(Language::German);
    state.set_language_preference(repo.load_language());
    assert_eq!(state.language, Language::German);

    repo.save_language(LanguagePreference::Fixed(Language::Italian));
    assert_eq!(
        repo.load_language(),
        LanguagePreference::Fixed(Language::Italian)
    );
    repo.save_language(LanguagePreference::System);
    assert_eq!(repo.load_language(), LanguagePreference::System);

    // A corrupt file falls back to the system language
    std::fs::write(format!("{dir}/language.cfg"), "??\n").unwrap();
    assert_eq!(repo.load_language(), LanguagePreference::System);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_settings_dpad_cycles_and_saves_language() {
    let dir = temp_test_dir("language-dpad");
    let repo = ConfigRepository::new(&dir);
    let mut state = settings_state(&dir);
    state.set_system_language(Language::Spanish);
    assert_eq!(state.language, Language::Spanish);

    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_LANGUAGE));

    // System (Español) -> English
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.language, Language::English);
    assert_eq!(
        repo.load_language(),
        LanguagePreference::Fixed(Language::English)
    );
    assert_eq!(state.toast_msg, Some(("Language: English".to_owned(), 110)));

    // ... -> Italiano -> back to the system language
    for _ in 0..4 {
        press(&mut state, &repo, BTN_CROSS);
    }
    assert_eq!(state.language, Language::Italian);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.language_pref, LanguagePreference::System);
    assert_eq!(state.language, Language::Spanish);
    assert_eq!(repo.load_language(), LanguagePreference::System);
    // The theme was not touched
    assert_eq!(
        state.theme_mode,
        vita_syncthing_app::domain::ThemeMode::Oled
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_settings_touch_on_language_card_cycles_language() {
    let dir = temp_test_dir("language-touch");
    let repo = ConfigRepository::new(&dir);
    let mut state = settings_state(&dir);

    let y = ((SETTINGS_LANGUAGE_Y + SETTINGS_LANGUAGE_B) / 2) as usize;
    InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_tap: Some((400, y)),
            any_activity: true,
            ..Default::default()
        },
    );
    assert_eq!(state.language, Language::English);
    assert_eq!(
        state.language_pref,
        LanguagePreference::Fixed(Language::English)
    );
    assert_eq!(
        repo.load_language(),
        LanguagePreference::Fixed(Language::English)
    );

    let _ = std::fs::remove_dir_all(&dir);
}
