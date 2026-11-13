//! Log viewer controller: scrolling of the full-screen protocol log and execution of its
//! `UiAction`s. The elements are declared by `ui/screens/logs.rs`.

use crate::domain::defaults::LOG_HISTORY_MAX_LINES;
use crate::domain::{AppState, LogScope, LogView, UiAction, LOG_VIEW_PAGE};
use crate::infrastructure::{
    ConfigRepository, BTN_CROSS, BTN_DOWN, BTN_L, BTN_LEFT, BTN_R, BTN_RIGHT, BTN_UP,
};

/// Buttons scrolling the viewer; they are removed from the frame once handled.
const SCROLL_BUTTONS: u32 = BTN_UP | BTN_DOWN | BTN_LEFT | BTN_RIGHT | BTN_L | BTN_R | BTN_CROSS;

pub struct LogsController;

impl LogsController {
    /// Scrolls the open viewer: `Up` / `Down` one line, `Left` / `Right` and `L` / `R` one
    /// page. Clears the handled buttons from `pressed` (`Cross` does nothing here).
    pub fn scroll(state: &mut AppState, pressed: &mut u32) {
        let buttons = *pressed & SCROLL_BUTTONS;
        *pressed &= !SCROLL_BUTTONS;
        let total = Self::line_count(state);
        let Some(view) = state.log_view.as_mut() else {
            return;
        };
        if buttons & BTN_UP != 0 {
            view.scroll_older(1, total);
        }
        if buttons & BTN_DOWN != 0 {
            view.scroll_newer(1, total);
        }
        if buttons & (BTN_LEFT | BTN_L) != 0 {
            view.scroll_older(LOG_VIEW_PAGE, total);
        }
        if buttons & (BTN_RIGHT | BTN_R) != 0 {
            view.scroll_newer(LOG_VIEW_PAGE, total);
        }
    }

    /// Executes a log viewer action (`OpenLogs` comes from the `Settings` log card).
    pub fn apply(state: &mut AppState, repo: &ConfigRepository, action: UiAction) {
        if action == UiAction::OpenLogs {
            state.log_view = Some(LogView::new());
            return;
        }
        let total = Self::line_count(state);
        let Some(view) = state.log_view.as_mut() else {
            return;
        };
        match action {
            UiAction::CloseLogs => state.log_view = None,
            UiAction::ToggleLogHistory => match view.scope {
                LogScope::Session => {
                    view.show_history(repo.read_log_history(LOG_HISTORY_MAX_LINES));
                }
                LogScope::History => view.show_session(),
            },
            UiAction::ScrollLogsOlder => view.scroll_older(LOG_VIEW_PAGE, total),
            UiAction::ScrollLogsNewer => view.scroll_newer(LOG_VIEW_PAGE, total),
            _ => {}
        }
    }

    fn line_count(state: &AppState) -> usize {
        state
            .log_view
            .as_ref()
            .map_or(0, |view| view.lines(&state.logs).len())
    }
}
