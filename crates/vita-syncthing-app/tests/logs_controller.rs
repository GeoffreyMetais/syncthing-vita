//! Log viewer controller (`LogsController`): scroll buttons (one line / one page), the buttons it
//! consumes from the frame, and the viewer actions (`OpenLogs`, `CloseLogs`, history toggle).

mod common;

use common::temp_test_dir;
use vita_syncthing_app::application::{LogsController, ShareService};
use vita_syncthing_app::domain::{AppState, LogView, UiAction, LOG_VIEW_PAGE, LOG_VIEW_ROWS};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_LEFT, BTN_R, BTN_RIGHT,
    BTN_TRIANGLE, BTN_UP,
};

/// State with `lines` session log lines and the viewer open on the newest ones.
fn setup(name: &str, lines: usize) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    for i in 0..lines {
        ShareService::log_event(&mut state, &repo, &format!("line {i}"));
    }
    LogsController::apply(&mut state, &repo, UiAction::OpenLogs);
    (dir, repo, state)
}

/// Scrolls the viewer with `buttons` and returns the buttons left in the frame.
fn scroll(state: &mut AppState, buttons: u32) -> u32 {
    let mut pressed = buttons;
    LogsController::scroll(state, &mut pressed);
    pressed
}

/// First visible line index of the open viewer.
fn top(state: &AppState) -> usize {
    let view = state.log_view.as_ref().expect("viewer open");
    view.window(view.lines(&state.logs).len()).0
}

fn view(state: &AppState) -> &LogView {
    state.log_view.as_ref().expect("viewer open")
}

#[test]
fn test_open_starts_on_the_newest_lines_and_close_removes_the_viewer() {
    let (dir, repo, mut state) = setup("logs-ctl-open", 40);
    assert_eq!(top(&state), 40 - LOG_VIEW_ROWS);
    LogsController::apply(&mut state, &repo, UiAction::CloseLogs);
    assert!(state.log_view.is_none());
    // Closing again, or other viewer actions without a viewer, change nothing.
    LogsController::apply(&mut state, &repo, UiAction::CloseLogs);
    LogsController::apply(&mut state, &repo, UiAction::ScrollLogsOlder);
    LogsController::apply(&mut state, &repo, UiAction::ToggleLogHistory);
    assert!(state.log_view.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_up_and_down_scroll_one_line_and_are_consumed() {
    let (dir, _repo, mut state) = setup("logs-ctl-line", 40);
    let start = top(&state);
    assert_eq!(scroll(&mut state, BTN_UP), 0);
    assert_eq!(top(&state), start - 1);
    assert_eq!(scroll(&mut state, BTN_DOWN), 0);
    assert_eq!(top(&state), start);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_left_and_l_scroll_one_page_older_and_right_and_r_one_page_newer() {
    let (dir, _repo, mut state) = setup("logs-ctl-page", 60);
    let start = top(&state);
    scroll(&mut state, BTN_LEFT);
    assert_eq!(top(&state), start - LOG_VIEW_PAGE);
    scroll(&mut state, BTN_L);
    assert_eq!(top(&state), start - 2 * LOG_VIEW_PAGE);
    scroll(&mut state, BTN_RIGHT);
    assert_eq!(top(&state), start - LOG_VIEW_PAGE);
    scroll(&mut state, BTN_R);
    assert_eq!(top(&state), start);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_scrolling_stops_at_both_ends_of_the_log() {
    let (dir, _repo, mut state) = setup("logs-ctl-ends", 30);
    for _ in 0..40 {
        scroll(&mut state, BTN_UP);
    }
    assert_eq!(top(&state), 0);
    for _ in 0..40 {
        scroll(&mut state, BTN_R);
    }
    assert_eq!(top(&state), 30 - LOG_VIEW_ROWS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_is_consumed_but_does_nothing_in_the_viewer() {
    let (dir, _repo, mut state) = setup("logs-ctl-cross", 40);
    let start = top(&state);
    assert_eq!(scroll(&mut state, BTN_CROSS), 0);
    assert_eq!(top(&state), start);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_other_buttons_are_left_in_the_frame() {
    let (dir, _repo, mut state) = setup("logs-ctl-passthrough", 40);
    let start = top(&state);
    // Circle (close) and Triangle (history) are for the viewer's hints, not for scrolling.
    assert_eq!(scroll(&mut state, BTN_CIRCLE | BTN_TRIANGLE), BTN_CIRCLE | BTN_TRIANGLE);
    assert_eq!(top(&state), start);
    // Scrolling buttons pressed with another one: only the scrolling ones are consumed.
    assert_eq!(scroll(&mut state, BTN_UP | BTN_CIRCLE), BTN_CIRCLE);
    assert_eq!(top(&state), start - 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_toggle_history_loads_syncthing_log_and_back_to_the_session() {
    let (dir, repo, mut state) = setup("logs-ctl-history", 5);
    // Earlier history written by a previous session into syncthing.log.
    repo.write_log("old entry 1");
    repo.write_log("old entry 2");
    LogsController::apply(&mut state, &repo, UiAction::ToggleLogHistory);
    assert!(view(&state).scope == vita_syncthing_app::domain::LogScope::History);
    let shown = view(&state).lines(&state.logs).len();
    assert!(shown >= 2, "history lists the file's lines");
    LogsController::apply(&mut state, &repo, UiAction::ToggleLogHistory);
    assert!(view(&state).scope == vita_syncthing_app::domain::LogScope::Session);
    assert_eq!(view(&state).lines(&state.logs).len(), 5);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_scroll_actions_move_by_a_page_from_the_hint_bar() {
    let (dir, repo, mut state) = setup("logs-ctl-actions", 60);
    let start = top(&state);
    LogsController::apply(&mut state, &repo, UiAction::ScrollLogsOlder);
    assert_eq!(top(&state), start - LOG_VIEW_PAGE);
    LogsController::apply(&mut state, &repo, UiAction::ScrollLogsNewer);
    assert_eq!(top(&state), start);
    // Actions that are not viewer actions are ignored by the viewer.
    LogsController::apply(&mut state, &repo, UiAction::Inert);
    assert_eq!(top(&state), start);
    assert!(state.log_view.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_face_buttons_drive_the_viewer_through_the_input_pipeline() {
    use common::press;
    let (dir, repo, mut state) = setup("logs-ctl-pipeline", 60);
    let start = top(&state);
    press(&mut state, &repo, BTN_UP);
    assert_eq!(top(&state), start - 1);
    press(&mut state, &repo, BTN_CIRCLE);
    assert!(state.log_view.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
