//! `Overview` page controller: D-pad focus over the hero card buttons. Its elements are
//! declared by `ui/screens/dashboard.rs`; their actions are global ones (`InputController`).

use crate::application::DpadNavigation;
use crate::domain::{action_count, ActiveTab, AppState, UiAction};

pub struct DashboardController;

impl DashboardController {
    /// Moves the D-pad focus along the `[QR / ID, Sync Now]` row.
    pub fn move_focus(state: &mut AppState, buttons: u32) -> Option<UiAction> {
        DpadNavigation::move_in_row(state, action_count(ActiveTab::Dashboard), buttons);
        None
    }
}
