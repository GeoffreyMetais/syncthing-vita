//! Background sync plugin model (`domain::plugin`): module locations, active `tai` directory,
//! stale entries, status resolution and the buttons of its `Settings` card.

use vita_syncthing_app::domain::plugin::{
    active_tai_dir, all_module_entries, stale_entries, LAUNCHER_MODULE, PLUGIN_BUNDLE_DIR,
    PLUGIN_MODULES, TAI_CONFIG_BACKUP_FILE, TAI_CONFIG_FILE, TAI_DIRS, TAI_DIR_UR0, TAI_DIR_UX0,
    WORKER_MODULE,
};
use vita_syncthing_app::domain::{AppState, PluginInfo, PluginStatus, SETTINGS_ROW_PLUGIN};

#[test]
fn test_module_and_directory_names_shared_with_the_plugin() {
    assert_eq!(LAUNCHER_MODULE, "syncthing_launcher.suprx");
    assert_eq!(WORKER_MODULE, "syncthing_worker.suprx");
    assert_eq!(PLUGIN_MODULES, [LAUNCHER_MODULE, WORKER_MODULE]);
    assert_eq!(PLUGIN_BUNDLE_DIR, "app0:plugins/");
    assert_eq!(TAI_DIRS, ["ux0:tai/", "ur0:tai/"]);
    assert_eq!(TAI_CONFIG_FILE, "config.txt");
    assert_eq!(TAI_CONFIG_BACKUP_FILE, "config.txt.bak");
}

#[test]
fn test_ux0_config_wins_over_ur0() {
    assert_eq!(active_tai_dir(true), TAI_DIR_UX0);
    assert_eq!(active_tai_dir(false), TAI_DIR_UR0);
}

#[test]
fn test_stale_entries_are_everything_but_the_active_launcher() {
    assert_eq!(
        stale_entries(TAI_DIR_UR0),
        [
            "ux0:tai/syncthing_launcher.suprx",
            "ux0:tai/syncthing_worker.suprx",
            "ur0:tai/syncthing_worker.suprx",
        ]
    );
    assert_eq!(
        stale_entries(TAI_DIR_UX0),
        [
            "ux0:tai/syncthing_worker.suprx",
            "ur0:tai/syncthing_launcher.suprx",
            "ur0:tai/syncthing_worker.suprx",
        ]
    );
    assert_eq!(
        all_module_entries(),
        [
            "ux0:tai/syncthing_launcher.suprx",
            "ux0:tai/syncthing_worker.suprx",
            "ur0:tai/syncthing_launcher.suprx",
            "ur0:tai/syncthing_worker.suprx",
        ]
    );
}

#[test]
fn test_status_resolution() {
    use PluginStatus::*;
    for alive in [false, true] {
        for enabled in [false, true] {
            assert_eq!(PluginStatus::resolve(false, alive, enabled), NotInstalled);
        }
    }
    assert_eq!(
        PluginStatus::resolve(true, false, true),
        InstalledNeedsReboot
    );
    assert_eq!(
        PluginStatus::resolve(true, false, false),
        InstalledNeedsReboot
    );
    assert_eq!(PluginStatus::resolve(true, true, true), Enabled);
    assert_eq!(PluginStatus::resolve(true, true, false), Disabled);
    assert!(!NotInstalled.is_installed());
    assert!(InstalledNeedsReboot.is_installed());
    assert!(Enabled.is_installed());
    assert!(Disabled.is_installed());
    assert_eq!(PluginStatus::default(), NotInstalled);
}

#[test]
fn test_plugin_info_unload_pending_and_buttons() {
    let info = PluginInfo::new(false, true, false);
    assert_eq!(info.status, PluginStatus::NotInstalled);
    assert!(info.launcher_loaded);
    assert!(!info.enabled);
    assert!(info.unload_pending(), "uninstalled, launcher still loaded");
    assert_eq!(info.action_count(), 1);

    assert!(!PluginInfo::new(false, false, true).unload_pending());
    let installed = PluginInfo::new(true, true, true);
    assert!(!installed.unload_pending());
    assert!(installed.enabled);
    assert_eq!(installed.action_count(), 2);
    assert_eq!(PluginInfo::new(true, false, true).action_count(), 2);
    assert_eq!(PluginInfo::default(), PluginInfo::new(false, false, false));
}

#[test]
fn test_settings_plugin_row_buttons_follow_the_status() {
    let mut state = AppState::new(Vec::new(), Vec::new(), "/tmp");
    assert_eq!(state.plugin, PluginInfo::default());
    assert_eq!(state.settings_row_actions(SETTINGS_ROW_PLUGIN), 1);
    state.plugin = PluginInfo::new(true, false, true);
    assert_eq!(state.settings_row_actions(SETTINGS_ROW_PLUGIN), 2);
    // Other cards keep their fixed number of buttons.
    assert_eq!(state.settings_row_actions(0), 1);
    assert_eq!(state.settings_row_actions(1), 2);
}
