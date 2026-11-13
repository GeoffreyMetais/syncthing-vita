//! D-pad navigation on the main screen (`DpadNavigation`): moves the focus between the sidebar
//! tabs and the current page's elements, and turns `Cross` into the focused element's action.
//!
//! How the focus moves within a page belongs to that page's controller; which element holds
//! the focus, and what it does, is declared by the screen in the frame's `HitMap`.

use crate::application::{DashboardController, FilesController, SettingsController};
use crate::domain::{ActiveTab, AppState, NavFocus, UiAction, TAB_ORDER};
use crate::infrastructure::{BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP};

/// Buttons handled here; they are removed from the frame so shortcuts do not see them.
const NAV_BUTTONS: u32 = BTN_UP | BTN_DOWN | BTN_LEFT | BTN_RIGHT | BTN_CROSS;

pub struct DpadNavigation;

impl DpadNavigation {
    /// Handles D-pad and `Cross` on the main screen (no modal open) and clears them from
    /// `pressed`. Returns the action to execute, if any.
    pub fn handle(state: &mut AppState, pressed: &mut u32) -> Option<UiAction> {
        let buttons = *pressed & NAV_BUTTONS;
        *pressed &= !NAV_BUTTONS;
        if buttons == 0 {
            return None;
        }
        if state.nav.focus == NavFocus::Sidebar {
            return Self::sidebar(state, buttons);
        }
        let moved = match state.active_tab {
            ActiveTab::Dashboard => DashboardController::move_focus(state, buttons),
            ActiveTab::FolderPicker => FilesController::move_focus(state, buttons),
            ActiveTab::Settings => SettingsController::move_focus(state, buttons),
        };
        if moved.is_some() {
            return moved;
        }
        // `Cross` activates the element holding the focus in the frame on screen.
        if buttons & BTN_CROSS != 0 && state.nav.focus == NavFocus::Content {
            return state.hit_map.focused_action(state.focus_slot());
        }
        None
    }

    fn sidebar(state: &mut AppState, buttons: u32) -> Option<UiAction> {
        if buttons & (BTN_RIGHT | BTN_CROSS) != 0 {
            state.nav.focus = NavFocus::Content;
            return None;
        }
        let pos = TAB_ORDER
            .iter()
            .position(|&t| t == state.active_tab)
            .unwrap_or(0);
        if buttons & BTN_UP != 0 && pos > 0 {
            return Some(UiAction::SwitchTab(TAB_ORDER[pos - 1]));
        }
        if buttons & BTN_DOWN != 0 && pos + 1 < TAB_ORDER.len() {
            return Some(UiAction::SwitchTab(TAB_ORDER[pos + 1]));
        }
        None
    }

    /// Moves along a row of `columns` buttons: `Left` on the first button goes back to the
    /// sidebar, `Right` stops at the last button.
    pub(crate) fn move_in_row(state: &mut AppState, columns: usize, buttons: u32) {
        if buttons & BTN_LEFT != 0 {
            if state.nav.col == 0 {
                state.nav.focus = NavFocus::Sidebar;
            } else {
                state.nav.col -= 1;
            }
        }
        if buttons & BTN_RIGHT != 0 && state.nav.col + 1 < columns {
            state.nav.col += 1;
        }
    }
}
