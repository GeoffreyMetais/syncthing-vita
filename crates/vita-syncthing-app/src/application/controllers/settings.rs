//! `Settings` page controller: D-pad focus over the cards and execution of the settings
//! `UiAction`s. The elements are declared by `ui/screens/settings.rs`.

use crate::application::{DpadNavigation, FolderOfferService};
use crate::domain::{
    default_col, settings_row_action_count, ActiveTab, AppState, NavFocus, UiAction, SETTINGS_ROWS,
};
use crate::infrastructure::{ConfigRepository, BTN_DOWN, BTN_LEFT, BTN_UP};

pub struct SettingsController;

impl SettingsController {
    /// Moves the D-pad focus between Settings cards and within multi-button cards.
    pub fn move_focus(state: &mut AppState, buttons: u32) -> Option<UiAction> {
        if buttons & BTN_UP != 0 && state.nav.row > 0 {
            state.nav.row -= 1;
            state.nav.col = default_col(ActiveTab::Settings);
        }
        if buttons & BTN_DOWN != 0 && state.nav.row + 1 < SETTINGS_ROWS {
            state.nav.row += 1;
            state.nav.col = 0;
        }
        let cols = settings_row_action_count(state.nav.row);
        if cols > 1 {
            DpadNavigation::move_in_row(state, cols, buttons);
        } else if buttons & BTN_LEFT != 0 {
            state.nav.focus = NavFocus::Sidebar;
        }
        None
    }

    /// Executes a `Settings` action; choices are persisted right away.
    pub fn apply(state: &mut AppState, repo: &ConfigRepository, action: UiAction) {
        match action {
            UiAction::ToggleTheme => {
                state.toggle_theme();
                repo.save_theme(state.theme_mode);
            }
            UiAction::CycleLanguage => {
                state.cycle_language();
                repo.save_language(state.language_pref);
            }
            UiAction::ClearDeclinedFolders => {
                FolderOfferService::clear_declined(state, repo);
            }
            _ => {}
        }
    }
}
