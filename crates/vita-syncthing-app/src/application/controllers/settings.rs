//! `Settings` page controller: D-pad focus over the cards and execution of the settings
//! `UiAction`s. The elements are declared by `ui/screens/settings.rs` (and
//! `settings_plugin.rs` for the background sync plugin card).

use crate::application::{DpadNavigation, FolderOfferService, UserCommand};
use crate::domain::{
    default_col, ActiveTab, AppState, ModalDialog, NavFocus, PluginRequest, UiAction, SETTINGS_ROWS,
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
        let cols = state.settings_row_actions(state.nav.row);
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

    /// Background sync plugin card: `Install` / `Uninstall` ask for confirmation first (the
    /// operation is carried out by `PluginService` once confirmed); `Enable / Disable` is
    /// applied right away. Elements of a stale frame (wrong status) are ignored.
    pub fn plugin_action(state: &mut AppState, action: UiAction) -> UserCommand {
        let installed = state.plugin.status.is_installed();
        match action {
            UiAction::InstallPlugin if !installed => {
                state.modal = ModalDialog::ConfirmPlugin {
                    request: PluginRequest::Install,
                };
            }
            UiAction::UninstallPlugin if installed => {
                state.modal = ModalDialog::ConfirmPlugin {
                    request: PluginRequest::Uninstall,
                };
            }
            UiAction::TogglePluginEnabled if installed => {
                return UserCommand::Plugin(PluginRequest::SetEnabled(!state.plugin.enabled));
            }
            _ => {}
        }
        UserCommand::None
    }
}
