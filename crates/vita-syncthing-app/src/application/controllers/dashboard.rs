//! `Overview` page controller: D-pad focus over the device card buttons (the page header) and
//! the share rows below it. The elements are declared by `ui/screens/dashboard.rs` and
//! `ui/screens/shares.rs`; their actions are executed by `InputController` (global ones) and
//! `SharesController` (share ones).

use crate::application::DpadNavigation;
use crate::domain::{action_count, ActiveTab, AppState, UiAction, DASHBOARD_HEADER_ACTIONS};
use crate::infrastructure::{BTN_DOWN, BTN_UP};

pub struct DashboardController;

impl DashboardController {
    /// Moves the D-pad focus: the `[QR / ID, New Share]` header row, then one row per share
    /// with its `[Devices, Delete, Folder]` buttons.
    pub fn move_focus(state: &mut AppState, buttons: u32) -> Option<UiAction> {
        let count = state.shares.len();
        if count == 0 {
            state.nav.header = true;
        }
        if buttons & BTN_UP != 0 && !state.nav.header {
            if state.selected_share_idx == 0 {
                state.nav.header = true;
            } else {
                state.selected_share_idx -= 1;
            }
        }
        if buttons & BTN_DOWN != 0 {
            if state.nav.header {
                state.nav.header = count == 0;
            } else if state.selected_share_idx + 1 < count {
                state.selected_share_idx += 1;
            }
        }
        if state.selected_share_idx >= count {
            state.selected_share_idx = count.saturating_sub(1);
        }
        let columns = if state.nav.header {
            DASHBOARD_HEADER_ACTIONS
        } else {
            action_count(ActiveTab::Dashboard)
        };
        state.nav.col = state.nav.col.min(columns - 1);
        DpadNavigation::move_in_row(state, columns, buttons);
        None
    }
}
