//! Colour theme setting: `Settings` toggle (D-pad, `Square`, touch) and `theme.cfg` persistence.

mod common;

use common::{press, rect_of, tap, tap_action, temp_test_dir};
use vita_syncthing_app::domain::{ActiveTab, AppState, NavFocus, ThemeMode, UiAction};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_CROSS, BTN_SQUARE};

#[test]
fn test_settings_cross_toggles_and_persists_theme() {
    let dir = temp_test_dir("theme-toggle");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // On Settings with the page focused, CROSS toggles OLED -> Dark and persists it
    state.active_tab = ActiveTab::Settings;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    assert_eq!(state.theme_mode, ThemeMode::Oled);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.theme_mode, ThemeMode::Dark);
    assert_eq!(repo.load_theme(), ThemeMode::Dark);

    // Starting a new session preserves the persisted theme
    let mut restarted = AppState::new(Vec::new(), Vec::new(), &dir);
    restarted.theme_mode = repo.load_theme();
    assert_eq!(restarted.theme_mode, ThemeMode::Dark);

    // Toggling again switches to SyncthingDark, then SyncthingLight, then back to OLED and persists
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.theme_mode, ThemeMode::SyncthingDark);
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingDark);

    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.theme_mode, ThemeMode::SyncthingLight);
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingLight);

    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.theme_mode, ThemeMode::Oled);
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_theme_persistence_roundtrip_and_touch() {
    let dir = temp_test_dir("theme-persistence");
    let repo = ConfigRepository::new(&dir);

    // Missing file defaults to OLED
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

    // Corrupted file content falls back to OLED
    std::fs::write(format!("{}/theme.cfg", dir), "invalid_theme_value\n").unwrap();
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.theme_mode = repo.load_theme();
    assert_eq!(state.theme_mode, ThemeMode::Oled);

    state.active_tab = ActiveTab::Settings;

    // 1. Touch on Settings display card toggles OLED -> Dark and persists
    // Inside the Display & Theme card, away from its buttons
    let card = rect_of(&mut state, UiAction::ToggleTheme);
    tap(
        &mut state,
        &repo,
        (card.x + 60) as usize,
        (card.y + card.h - 12) as usize,
    );
    assert_eq!(state.theme_mode, ThemeMode::Dark);
    assert_eq!(repo.load_theme(), ThemeMode::Dark);

    // 2. Square button on Settings tab toggles Dark -> SyncthingDark and persists
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(state.theme_mode, ThemeMode::SyncthingDark);
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingDark);

    // 3. Touch on the `Switch Theme` button toggles SyncthingDark -> SyncthingLight and persists
    tap_action(&mut state, &repo, UiAction::ToggleTheme);
    assert_eq!(state.theme_mode, ThemeMode::SyncthingLight);
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingLight);

    // 4. Starting a new session preserves the persisted theme
    let mut new_session = AppState::new(Vec::new(), Vec::new(), &dir);
    new_session.theme_mode = repo.load_theme();
    assert_eq!(new_session.theme_mode, ThemeMode::SyncthingLight);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_syncthing_themes_radii_contrast_and_render() {
    use vita_syncthing_app::domain::ModalDialog;
    use vita_syncthing_app::ui::gfx::{Color, Gfx};
    use vita_syncthing_app::ui::renderer::draw_frame;
    use vita_syncthing_app::ui::theme::UiTheme;

    let luma = |c: Color| -> u32 {
        let r = c & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = (c >> 16) & 0xFF;
        299 * r + 587 * g + 114 * b
    };

    let dark = UiTheme::for_mode(ThemeMode::Dark);
    let oled = UiTheme::for_mode(ThemeMode::Oled);
    let st_dark = UiTheme::for_mode(ThemeMode::SyncthingDark);
    let st_light = UiTheme::for_mode(ThemeMode::SyncthingLight);

    // Syncthing themes use crisper, less rounded Bootstrap-style buttons and cards
    assert!(st_dark.button_radius < dark.button_radius);
    assert!(st_dark.button_radius < oled.button_radius);
    assert!(st_dark.card_radius < dark.card_radius);
    assert!(st_dark.card_radius < oled.card_radius);

    assert!(st_light.button_radius < dark.button_radius);
    assert!(st_light.button_radius < oled.button_radius);
    assert!(st_light.card_radius < dark.card_radius);
    assert!(st_light.card_radius < oled.card_radius);

    // SyncthingLight has a light background (bg brighter than text), SyncthingDark has a dark background
    assert!(luma(st_light.bg) > luma(st_light.text));
    assert!(luma(st_dark.text) > luma(st_dark.bg));

    // Render frames cleanly across tabs and modals in both Syncthing themes
    let dir = temp_test_dir("syncthing-theme-render");
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let mut gfx = Gfx::new();
    for mode in [ThemeMode::SyncthingDark, ThemeMode::SyncthingLight] {
        state.theme_mode = mode;
        for tab in [
            ActiveTab::Dashboard,
            ActiveTab::FolderPicker,
            ActiveTab::Settings,
        ] {
            state.active_tab = tab;
            state.modal = ModalDialog::None;
            draw_frame(&mut gfx, &mut state);
            assert!(!state.hit_map.hotspots().is_empty());
        }
        state.modal = ModalDialog::ConfirmQuit;
        draw_frame(&mut gfx, &mut state);
        assert!(!state.hit_map.hotspots().is_empty());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
