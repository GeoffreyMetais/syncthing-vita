//! `Settings` background sync plugin card: D-pad focus over its buttons (one before install,
//! two after), `Install` / `Uninstall` through the `ConfirmPlugin` modal (pad and touch),
//! `Enable / Disable` applied at once, stale elements ignored, and the whole flow with
//! `PluginService`.

mod common;

use common::plugin::{host, seed, STOCK_CONFIG};
use common::{press, rects_of, tap_action, tap_rect, temp_test_dir};
use vita_syncthing_app::application::{PluginService, SettingsController, UserCommand};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, ModalDialog, NavFocus, PadKey, PluginInfo, PluginRequest, PluginStatus,
    UiAction, SETTINGS_ROW_LOG, SETTINGS_ROW_PLUGIN,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_START, BTN_UP,
};

/// `Settings` page, content focused, plugin `info`.
fn setup(name: &str, info: PluginInfo) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&format!("{dir}/data"));
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.active_tab = ActiveTab::Settings;
    state.reset_nav_for_tab();
    state.nav.focus = NavFocus::Content;
    state.plugin = info;
    (dir, repo, state)
}

fn not_installed() -> PluginInfo {
    PluginInfo::new(false, false, true)
}

fn enabled() -> PluginInfo {
    PluginInfo::new(true, true, true)
}

/// Moves the D-pad focus down to the plugin card.
fn focus_plugin_row(state: &mut AppState, repo: &ConfigRepository) {
    for _ in 0..SETTINGS_ROW_PLUGIN {
        press(state, repo, BTN_DOWN);
    }
    assert!(state.is_settings_row_focused(SETTINGS_ROW_PLUGIN));
}

#[test]
fn test_plugin_card_sits_between_declined_folders_and_the_log() {
    let (dir, repo, mut state) = setup("settings-plugin-row", not_installed());
    focus_plugin_row(&mut state, &repo);
    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_LOG));
    press(&mut state, &repo, BTN_UP);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_PLUGIN));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_install_button_opens_the_confirmation_then_cross_confirms() {
    let (dir, repo, mut state) = setup("settings-plugin-install", not_installed());
    focus_plugin_row(&mut state, &repo);
    // A single button: Right stays on it, Left goes back to the sidebar.
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 0);
    assert_eq!(press(&mut state, &repo, BTN_CROSS), UserCommand::None);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmPlugin {
            request: PluginRequest::Install
        }
    );
    assert_eq!(
        press(&mut state, &repo, BTN_CROSS),
        UserCommand::Plugin(PluginRequest::Install)
    );
    assert_eq!(state.modal, ModalDialog::None);

    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_confirmation_is_cancelled_by_circle_start_and_outside_touch() {
    let (dir, repo, mut state) = setup("settings-plugin-cancel", not_installed());
    for cancel in [BTN_CIRCLE, BTN_START] {
        tap_action(&mut state, &repo, UiAction::InstallPlugin);
        assert!(matches!(state.modal, ModalDialog::ConfirmPlugin { .. }));
        assert_eq!(press(&mut state, &repo, cancel), UserCommand::None);
        assert_eq!(state.modal, ModalDialog::None);
    }
    tap_action(&mut state, &repo, UiAction::InstallPlugin);
    // Other buttons do nothing.
    assert_eq!(press(&mut state, &repo, BTN_UP), UserCommand::None);
    assert!(matches!(state.modal, ModalDialog::ConfirmPlugin { .. }));
    // Touch outside the card cancels.
    assert_eq!(common::tap(&mut state, &repo, 4, 4), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_confirmation_buttons_and_hints_answer_touch() {
    let (dir, repo, mut state) = setup("settings-plugin-touch", not_installed());
    tap_action(&mut state, &repo, UiAction::InstallPlugin);
    // The `Cross` button and its hint both confirm.
    let confirms = rects_of(&mut state, UiAction::Press(PadKey::Cross));
    assert_eq!(confirms.len(), 2, "button and hint");
    for rect in confirms {
        state.modal = ModalDialog::ConfirmPlugin {
            request: PluginRequest::Install,
        };
        assert_eq!(
            tap_rect(&mut state, &repo, rect),
            UserCommand::Plugin(PluginRequest::Install)
        );
        assert_eq!(state.modal, ModalDialog::None);
    }
    state.modal = ModalDialog::ConfirmPlugin {
        request: PluginRequest::Install,
    };
    let cancel = *rects_of(&mut state, UiAction::Press(PadKey::Circle))
        .last()
        .unwrap();
    assert_eq!(tap_rect(&mut state, &repo, cancel), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_installed_card_toggle_then_uninstall_with_the_dpad() {
    let (dir, repo, mut state) = setup("settings-plugin-installed", enabled());
    focus_plugin_row(&mut state, &repo);
    assert_eq!(state.nav.col, 0);
    assert_eq!(
        press(&mut state, &repo, BTN_CROSS),
        UserCommand::Plugin(PluginRequest::SetEnabled(false))
    );
    assert_eq!(
        state.modal,
        ModalDialog::None,
        "no confirmation for the toggle"
    );
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 1);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 1, "Right stops on the last button");
    assert_eq!(press(&mut state, &repo, BTN_CROSS), UserCommand::None);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmPlugin {
            request: PluginRequest::Uninstall
        }
    );
    assert_eq!(
        press(&mut state, &repo, BTN_CROSS),
        UserCommand::Plugin(PluginRequest::Uninstall)
    );
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.col, 0);
    press(&mut state, &repo, BTN_LEFT);
    assert_eq!(state.nav.focus, NavFocus::Sidebar);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_disabled_card_toggle_enables_by_touch() {
    let (dir, repo, mut state) = setup(
        "settings-plugin-disabled",
        PluginInfo::new(true, true, false),
    );
    assert_eq!(state.plugin.status, PluginStatus::Disabled);
    assert_eq!(
        tap_action(&mut state, &repo, UiAction::TogglePluginEnabled),
        UserCommand::Plugin(PluginRequest::SetEnabled(true))
    );
    // Not installed: no toggle and no uninstall on screen, only Install.
    state.plugin = not_installed();
    assert!(rects_of(&mut state, UiAction::TogglePluginEnabled).is_empty());
    assert!(rects_of(&mut state, UiAction::UninstallPlugin).is_empty());
    assert_eq!(rects_of(&mut state, UiAction::InstallPlugin).len(), 1);
    // Installed (any status): no Install button.
    for info in [
        PluginInfo::new(true, false, true),
        enabled(),
        PluginInfo::new(true, true, false),
    ] {
        state.plugin = info;
        assert!(rects_of(&mut state, UiAction::InstallPlugin).is_empty());
        assert_eq!(rects_of(&mut state, UiAction::UninstallPlugin).len(), 1);
        assert_eq!(rects_of(&mut state, UiAction::TogglePluginEnabled).len(), 1);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_focus_column_is_clamped_when_the_card_loses_its_second_button() {
    let (dir, repo, mut state) = setup("settings-plugin-clamp", enabled());
    focus_plugin_row(&mut state, &repo);
    press(&mut state, &repo, BTN_RIGHT);
    assert_eq!(state.nav.col, 1);
    // Uninstalled meanwhile: `Cross` activates the single `Install` button.
    state.plugin = not_installed();
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmPlugin {
            request: PluginRequest::Install
        }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_stale_actions_are_ignored() {
    let (dir, _repo, mut state) = setup("settings-plugin-stale", not_installed());
    for action in [UiAction::UninstallPlugin, UiAction::TogglePluginEnabled] {
        assert_eq!(
            SettingsController::plugin_action(&mut state, action),
            UserCommand::None
        );
        assert_eq!(state.modal, ModalDialog::None);
    }
    state.plugin = enabled();
    assert_eq!(
        SettingsController::plugin_action(&mut state, UiAction::InstallPlugin),
        UserCommand::None
    );
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(
        SettingsController::plugin_action(&mut state, UiAction::OpenLogs),
        UserCommand::None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_install_toggle_uninstall_end_to_end() {
    let (dir, repo, mut state) = setup("settings-plugin-e2e", PluginInfo::default());
    seed(&dir, STOCK_CONFIG);
    let host = host(&dir, false);
    PluginService::refresh(&mut state, &host);
    let run = |state: &mut AppState, cmd: UserCommand| {
        if let UserCommand::Plugin(request) = cmd {
            PluginService::execute(state, &repo, &host, request);
        }
    };

    tap_action(&mut state, &repo, UiAction::InstallPlugin);
    let cmd = press(&mut state, &repo, BTN_CROSS);
    run(&mut state, cmd);
    assert_eq!(state.plugin.status, PluginStatus::InstalledNeedsReboot);

    let cmd = tap_action(&mut state, &repo, UiAction::TogglePluginEnabled);
    run(&mut state, cmd);
    assert!(!state.plugin.enabled);
    assert_eq!(state.plugin.status, PluginStatus::InstalledNeedsReboot);

    tap_action(&mut state, &repo, UiAction::UninstallPlugin);
    let cmd = press(&mut state, &repo, BTN_CROSS);
    run(&mut state, cmd);
    assert_eq!(state.plugin.status, PluginStatus::NotInstalled);
    let _ = std::fs::remove_dir_all(&dir);
}
