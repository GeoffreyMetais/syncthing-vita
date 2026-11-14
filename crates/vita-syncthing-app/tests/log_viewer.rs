//! Full-screen protocol log viewer: opened from the `Settings` log card (touch or D-pad),
//! scrolling (D-pad, `L` / `R`, touch), `Show All` history from `syncthing.log`, and its
//! geometry in every language.

mod common;

use common::{draw, press, rects_of, tap_action, tap_rect, temp_test_dir};
use vita_syncthing_app::application::ShareService;
use vita_syncthing_app::domain::defaults::LOG_BUFFER_LINES;
use vita_syncthing_app::domain::{
    ActiveTab, AppState, Language, LanguagePreference, LogScope, LogView, NavFocus, UiAction,
    LOG_VIEW_PAGE, LOG_VIEW_ROWS, SETTINGS_ROW_LOG,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_R, BTN_TRIANGLE, BTN_UP,
};
use vita_syncthing_app::ui::gfx::{GFX_H, GFX_W};
use vita_syncthing_app::ui::theme::HINT_Y;

const LANGUAGES: [Language; 5] = [
    Language::English,
    Language::French,
    Language::Spanish,
    Language::German,
    Language::Italian,
];

/// State on the `Settings` tab with `lines` lines logged in this session.
fn setup(name: &str, lines: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    for i in 0..lines {
        ShareService::log_event(&mut state, &repo, &format!("line {i}"));
    }
    tap_action(&mut state, &repo, UiAction::SwitchTab(ActiveTab::Settings));
    (dir, repo, state)
}

fn view(state: &AppState) -> &LogView {
    state.log_view.as_ref().expect("log viewer open")
}

/// Visible range of the open viewer.
fn window(state: &AppState) -> (usize, usize) {
    let v = view(state);
    v.window(v.lines(&state.logs).len())
}

#[test]
fn test_touching_the_log_card_opens_the_viewer_and_circle_closes_it() {
    let (dir, repo, mut state) = setup("logs-open-touch", 3);
    // The whole card is a touch target, not only its `Open` button.
    let card = rects_of(&mut state, UiAction::OpenLogs)[0];
    tap_rect(&mut state, &repo, card);
    assert!(state.log_view.is_some());
    assert_eq!(view(&state).scope, LogScope::Session);

    // The viewer covers the sidebar: no tab is reachable while it is open.
    draw(&mut state);
    assert!(!state
        .hit_map
        .hotspots()
        .iter()
        .any(|h| matches!(h.action, UiAction::SwitchTab(_))));
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.active_tab, ActiveTab::Settings);

    press(&mut state, &repo, BTN_CIRCLE);
    assert!(state.log_view.is_none());
    assert_eq!(state.active_tab, ActiveTab::Settings);

    // Back on Settings: touching the `Back` hint of the viewer works too.
    tap_action(&mut state, &repo, UiAction::OpenLogs);
    tap_action(&mut state, &repo, UiAction::CloseLogs);
    assert!(state.log_view.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_dpad_reaches_the_log_card_open_button() {
    let (dir, repo, mut state) = setup("logs-open-dpad", 3);
    state.nav.focus = NavFocus::Content;
    for _ in 0..SETTINGS_ROW_LOG {
        press(&mut state, &repo, BTN_DOWN);
    }
    assert!(state.is_settings_row_focused(SETTINGS_ROW_LOG));
    press(&mut state, &repo, BTN_CROSS);
    assert!(state.log_view.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_viewer_scrolls_by_line_and_page_and_stays_in_bounds() {
    let total = 60;
    let (dir, repo, mut state) = setup("logs-scroll", total);
    tap_action(&mut state, &repo, UiAction::OpenLogs);
    // Opens on the newest lines.
    assert_eq!(window(&state), (total - LOG_VIEW_ROWS, total));

    press(&mut state, &repo, BTN_UP);
    assert_eq!(window(&state), (total - LOG_VIEW_ROWS - 1, total - 1));
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(window(&state), (total - LOG_VIEW_ROWS, total));
    // Already at the newest line: `Down` stays there.
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(window(&state), (total - LOG_VIEW_ROWS, total));

    press(&mut state, &repo, BTN_L);
    assert_eq!(
        window(&state),
        (total - LOG_VIEW_ROWS - LOG_VIEW_PAGE, total - LOG_VIEW_PAGE)
    );
    press(&mut state, &repo, BTN_R);
    assert_eq!(window(&state), (total - LOG_VIEW_ROWS, total));

    // Paging past the oldest line stops on it.
    for _ in 0..10 {
        press(&mut state, &repo, BTN_L);
    }
    assert_eq!(window(&state), (0, LOG_VIEW_ROWS));
    press(&mut state, &repo, BTN_DOWN);
    assert_eq!(window(&state), (1, LOG_VIEW_ROWS + 1));

    // Touching the upper / lower half of the list pages too.
    tap_action(&mut state, &repo, UiAction::ScrollLogsNewer);
    assert_eq!(
        window(&state),
        (1 + LOG_VIEW_PAGE, LOG_VIEW_ROWS + 1 + LOG_VIEW_PAGE)
    );
    tap_action(&mut state, &repo, UiAction::ScrollLogsOlder);
    assert_eq!(window(&state), (1, LOG_VIEW_ROWS + 1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_new_lines_follow_at_the_bottom_and_keep_a_scrolled_back_window() {
    let total = 40;
    let (dir, repo, mut state) = setup("logs-follow", total);
    tap_action(&mut state, &repo, UiAction::OpenLogs);
    ShareService::log_event(&mut state, &repo, "new line");
    assert_eq!(window(&state), (total + 1 - LOG_VIEW_ROWS, total + 1));

    press(&mut state, &repo, BTN_L);
    let before = window(&state);
    let first = state.logs[before.0].clone();
    ShareService::log_event(&mut state, &repo, "another line");
    let after = window(&state);
    assert_eq!(
        state.logs[after.0], first,
        "the lines being read stay on screen"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_show_all_loads_the_log_history_and_toggles_back() {
    let dir = temp_test_dir("logs-history");
    // Lines logged by previous runs of the app.
    let old: String = (0..30)
        .map(|i| format!("[VITA-SYNCTHING] old run {i}\n"))
        .collect();
    std::fs::write(format!("{dir}/syncthing.log"), old).unwrap();
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::log_event(&mut state, &repo, "this run");
    tap_action(&mut state, &repo, UiAction::SwitchTab(ActiveTab::Settings));
    tap_action(&mut state, &repo, UiAction::OpenLogs);
    assert_eq!(view(&state).lines(&state.logs).len(), 1);

    press(&mut state, &repo, BTN_TRIANGLE);
    let v = view(&state);
    assert_eq!(v.scope, LogScope::History);
    let lines = v.lines(&state.logs);
    assert_eq!(lines.len(), 31);
    assert_eq!(lines[0], "old run 0", "the file prefix is not displayed");
    assert_eq!(lines[30], "this run");

    // Lines logged while the history is shown are appended to it.
    ShareService::log_event(&mut state, &repo, "while viewing");
    assert_eq!(
        view(&state).lines(&state.logs).last().unwrap(),
        "while viewing"
    );

    tap_action(&mut state, &repo, UiAction::ToggleLogHistory);
    let v = view(&state);
    assert_eq!(v.scope, LogScope::Session);
    assert!(v.history.is_empty());
    assert_eq!(v.lines(&state.logs).len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_session_buffer_keeps_the_latest_lines() {
    let (dir, _repo, state) = setup("logs-buffer", LOG_BUFFER_LINES + 5);
    assert_eq!(state.logs.len(), LOG_BUFFER_LINES);
    assert_eq!(state.logs[0], "line 5");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_viewer_geometry_in_every_language() {
    let (dir, repo, mut state) = setup("logs-geometry", 50);
    for language in LANGUAGES {
        state.set_language_preference(LanguagePreference::Fixed(language));
        for history in [false, true] {
            state.log_view = None;
            tap_action(&mut state, &repo, UiAction::OpenLogs);
            if history {
                tap_action(&mut state, &repo, UiAction::ToggleLogHistory);
            }
            draw(&mut state);
            let hotspots = state.hit_map.hotspots().to_vec();
            for h in &hotspots {
                let r = h.rect;
                assert!(
                    r.x >= 0 && r.y >= 0 && r.x + r.w <= GFX_W && r.y + r.h <= GFX_H,
                    "{:?} is off screen in {language:?}: {r:?}",
                    h.action
                );
            }
            let hints: Vec<_> = hotspots.iter().filter(|h| h.rect.y >= HINT_Y).collect();
            for action in [
                UiAction::ScrollLogsOlder,
                UiAction::ScrollLogsNewer,
                UiAction::ToggleLogHistory,
                UiAction::CloseLogs,
            ] {
                assert!(
                    hints.iter().any(|h| h.action == action),
                    "{action:?} hint missing in {language:?}"
                );
            }
            for (i, a) in hints.iter().enumerate() {
                for b in &hints[i + 1..] {
                    let (a, b) = (a.rect, b.rect);
                    assert!(
                        a.x + a.w <= b.x || b.x + b.w <= a.x,
                        "hints overlap in {language:?}: {a:?} / {b:?}"
                    );
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
