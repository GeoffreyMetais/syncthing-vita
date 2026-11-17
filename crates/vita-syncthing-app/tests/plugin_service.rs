//! `PluginService`: install into the active `tai` directory (`ux0:` wins over `ur0:`),
//! `config.txt` backup and idempotent `*main` entry, `enabled=1` at install, uninstall from
//! both directories, `Enable / Disable` without reboot, status transitions, failures (no
//! bundled modules, no taiHEN, unwritable `tai`) and their toasts.

mod common;

use common::plugin::{
    exists, host, put, root, seed, text, LAUNCHER_BYTES, STOCK_CONFIG, WORKER_BYTES,
};
use common::temp_test_dir;
use vita_syncthing_app::application::{PluginError, PluginService};
use vita_syncthing_app::domain::{
    AppState, PluginInfo, PluginNotice, PluginRequest, PluginStatus, UiText,
};
use vita_syncthing_app::infrastructure::ConfigRepository;

const UR0_LAUNCHER: &str = "ur0:tai/syncthing_launcher.suprx";
const UX0_LAUNCHER: &str = "ux0:tai/syncthing_launcher.suprx";

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&format!("{dir}/data"));
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    (dir, repo, state)
}

fn plugin_cfg(dir: &str) -> String {
    std::fs::read_to_string(format!("{dir}/data/plugin.cfg")).unwrap_or_default()
}

fn toast(state: &AppState) -> Option<UiText> {
    state.toast_msg.as_ref().map(|(t, _)| t.clone())
}

#[test]
fn test_install_into_ur0_backs_up_config_and_enables() {
    let dir = temp_test_dir("plugin-install-ur0");
    seed(&dir, STOCK_CONFIG);
    let root = root(&dir);
    let host = host(&dir, false);
    assert_eq!(PluginService::active_tai_dir(&host), "ur0:tai/");
    assert_eq!(PluginService::install(&host), Ok("ur0:tai/"));

    assert_eq!(
        std::fs::read(common::plugin::local(&root, UR0_LAUNCHER)).unwrap(),
        LAUNCHER_BYTES
    );
    assert_eq!(
        std::fs::read(common::plugin::local(
            &root,
            "ur0:tai/syncthing_worker.suprx"
        ))
        .unwrap(),
        WORKER_BYTES
    );
    assert_eq!(
        text(&root, "ur0:tai/config.txt").unwrap(),
        format!("{STOCK_CONFIG}{UR0_LAUNCHER}\n")
    );
    assert_eq!(
        text(&root, "ur0:tai/config.txt.bak").as_deref(),
        Some(STOCK_CONFIG)
    );
    // The worker is never listed: the launcher loads it from its own directory.
    assert!(!text(&root, "ur0:tai/config.txt")
        .unwrap()
        .contains("syncthing_worker"));
    assert_eq!(
        plugin_cfg(&dir),
        "enabled=1\nsync_on_game_exit=1\ninterval_secs=1800\n"
    );
    assert!(!exists(&root, "ux0:tai/syncthing_launcher.suprx"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_install_prefers_ux0_and_drops_stale_ur0_entries() {
    let dir = temp_test_dir("plugin-install-ux0");
    seed(&dir, STOCK_CONFIG);
    let root = root(&dir);
    let ux0_config = format!("*main\r\n{UR0_LAUNCHER}\r\nux0:tai/other.suprx\r\n");
    put(&root, "ux0:tai/config.txt", ux0_config.as_bytes());
    let host = host(&dir, false);
    assert_eq!(PluginService::active_tai_dir(&host), "ux0:tai/");
    assert_eq!(PluginService::install(&host), Ok("ux0:tai/"));
    assert_eq!(
        text(&root, "ux0:tai/config.txt").unwrap(),
        format!("*main\r\nux0:tai/other.suprx\r\n{UX0_LAUNCHER}\r\n")
    );
    assert_eq!(
        text(&root, "ux0:tai/config.txt.bak").as_deref(),
        Some(ux0_config.as_str())
    );
    assert!(exists(&root, UX0_LAUNCHER));
    assert!(exists(&root, "ux0:tai/syncthing_worker.suprx"));
    // The inactive `ur0:` configuration is not touched.
    assert_eq!(text(&root, "ur0:tai/config.txt").unwrap(), STOCK_CONFIG);
    assert!(!exists(&root, "ur0:tai/config.txt.bak"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_reinstall_is_idempotent_and_keeps_the_first_backup() {
    let dir = temp_test_dir("plugin-reinstall");
    seed(&dir, STOCK_CONFIG);
    let root = root(&dir);
    let host = host(&dir, false);
    PluginService::install(&host).unwrap();
    let installed = text(&root, "ur0:tai/config.txt").unwrap();
    // A second install does not rewrite config.txt, so the backup is the original file.
    PluginService::install(&host).unwrap();
    assert_eq!(text(&root, "ur0:tai/config.txt").unwrap(), installed);
    assert_eq!(
        text(&root, "ur0:tai/config.txt.bak").as_deref(),
        Some(STOCK_CONFIG)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_install_keeps_plugin_cfg_settings_and_turns_it_back_on() {
    let dir = temp_test_dir("plugin-install-cfg");
    seed(&dir, STOCK_CONFIG);
    std::fs::create_dir_all(format!("{dir}/data")).unwrap();
    std::fs::write(
        format!("{dir}/data/plugin.cfg"),
        "enabled=0\ninterval_secs=900\nwifi=1\n",
    )
    .unwrap();
    PluginService::install(&host(&dir, false)).unwrap();
    assert_eq!(
        plugin_cfg(&dir),
        "enabled=1\nsync_on_game_exit=1\ninterval_secs=900\nwifi=1\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_install_without_bundled_modules_writes_nothing() {
    for missing in ["syncthing_launcher.suprx", "syncthing_worker.suprx"] {
        let dir = temp_test_dir("plugin-no-bundle");
        seed(&dir, STOCK_CONFIG);
        let root = root(&dir);
        std::fs::remove_file(common::plugin::local(
            &root,
            &format!("app0:plugins/{missing}"),
        ))
        .unwrap();
        let host = host(&dir, false);
        assert_eq!(
            PluginService::install(&host),
            Err(PluginError::BundleMissing(format!(
                "app0:plugins/{missing}"
            )))
        );
        assert_eq!(text(&root, "ur0:tai/config.txt").unwrap(), STOCK_CONFIG);
        assert!(!exists(&root, UR0_LAUNCHER));
        assert!(!exists(&root, "ur0:tai/config.txt.bak"));
        assert_eq!(plugin_cfg(&dir), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
    // An empty module is not a module.
    let dir = temp_test_dir("plugin-empty-bundle");
    seed(&dir, STOCK_CONFIG);
    put(&root(&dir), "app0:plugins/syncthing_worker.suprx", b"");
    assert!(matches!(
        PluginService::install(&host(&dir, false)),
        Err(PluginError::BundleMissing(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_install_without_taihen_config_writes_nothing() {
    let dir = temp_test_dir("plugin-no-tai");
    seed(&dir, STOCK_CONFIG);
    let root = root(&dir);
    std::fs::remove_file(common::plugin::local(&root, "ur0:tai/config.txt")).unwrap();
    assert_eq!(
        PluginService::install(&host(&dir, false)),
        Err(PluginError::TaiConfigMissing)
    );
    assert!(!exists(&root, UR0_LAUNCHER));
    assert_eq!(plugin_cfg(&dir), "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_install_into_unwritable_tai_reports_the_path() {
    let dir = temp_test_dir("plugin-unwritable");
    seed(&dir, STOCK_CONFIG);
    // A directory where the launcher's temp file should go: the copy cannot be written.
    std::fs::create_dir_all(format!(
        "{}/ur0/tai/syncthing_launcher.suprx.tmp/x",
        root(&dir)
    ))
    .unwrap();
    match PluginService::install(&host(&dir, false)) {
        Err(PluginError::Io(path, _)) => assert_eq!(path, UR0_LAUNCHER),
        other => panic!("unexpected {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_uninstall_cleans_both_tai_directories_and_disables() {
    let dir = temp_test_dir("plugin-uninstall");
    seed(&dir, STOCK_CONFIG);
    let root = root(&dir);
    let host = host(&dir, false);
    PluginService::install(&host).unwrap();
    // A stale copy in `ux0:tai/` (inactive: no config.txt there) is removed too.
    put(&root, UX0_LAUNCHER, b"old");
    put(&root, "ux0:tai/syncthing_worker.suprx", b"old");
    PluginService::uninstall(&host).unwrap();
    assert_eq!(text(&root, "ur0:tai/config.txt").unwrap(), STOCK_CONFIG);
    assert_eq!(
        text(&root, "ur0:tai/config.txt.bak").unwrap(),
        format!("{STOCK_CONFIG}{UR0_LAUNCHER}\n")
    );
    for dir_ in ["ux0:tai/", "ur0:tai/"] {
        for module in ["syncthing_launcher.suprx", "syncthing_worker.suprx"] {
            assert!(!exists(&root, &format!("{dir_}{module}")), "{dir_}{module}");
        }
    }
    assert!(plugin_cfg(&dir).starts_with("enabled=0\n"));
    // Nothing to remove: still fine, and config.txt is not rewritten.
    PluginService::uninstall(&host).unwrap();
    assert_eq!(text(&root, "ur0:tai/config.txt").unwrap(), STOCK_CONFIG);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_uninstall_removes_entries_from_both_configs() {
    let dir = temp_test_dir("plugin-uninstall-both");
    seed(&dir, &format!("*main\n{UR0_LAUNCHER}\n"));
    let root = root(&dir);
    put(
        &root,
        "ux0:tai/config.txt",
        format!("*main\n{UX0_LAUNCHER}\n").as_bytes(),
    );
    PluginService::uninstall(&host(&dir, true)).unwrap();
    assert_eq!(text(&root, "ur0:tai/config.txt").unwrap(), "*main\n");
    assert_eq!(text(&root, "ux0:tai/config.txt").unwrap(), "*main\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_status_needs_the_file_and_the_main_entry() {
    let dir = temp_test_dir("plugin-status");
    seed(&dir, STOCK_CONFIG);
    let root = root(&dir);
    let host = host(&dir, false);
    assert_eq!(
        PluginService::status(&host),
        PluginInfo::new(false, false, true)
    );
    // Listed but missing file.
    put(
        &root,
        "ur0:tai/config.txt",
        format!("*main\n{UR0_LAUNCHER}\n").as_bytes(),
    );
    assert_eq!(
        PluginService::status(&host).status,
        PluginStatus::NotInstalled
    );
    // File but not listed.
    put(&root, "ur0:tai/config.txt", STOCK_CONFIG.as_bytes());
    put(&root, UR0_LAUNCHER, LAUNCHER_BYTES);
    assert_eq!(
        PluginService::status(&host).status,
        PluginStatus::NotInstalled
    );
    // Both, but in the inactive directory once `ux0:tai/config.txt` exists.
    put(
        &root,
        "ur0:tai/config.txt",
        format!("*main\n{UR0_LAUNCHER}\n").as_bytes(),
    );
    assert_eq!(
        PluginService::status(&host).status,
        PluginStatus::InstalledNeedsReboot
    );
    put(&root, "ux0:tai/config.txt", b"*main\n");
    assert_eq!(
        PluginService::status(&host).status,
        PluginStatus::NotInstalled
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_lifecycle_through_execute() {
    let (dir, repo, mut state) = setup("plugin-lifecycle");
    seed(&dir, STOCK_CONFIG);
    let before_boot = host(&dir, false);
    PluginService::refresh(&mut state, &before_boot);
    assert_eq!(state.plugin.status, PluginStatus::NotInstalled);

    PluginService::execute(&mut state, &repo, &before_boot, PluginRequest::Install);
    assert_eq!(state.plugin.status, PluginStatus::InstalledNeedsReboot);
    assert!(state.plugin.enabled);
    assert_eq!(
        toast(&state),
        Some(UiText::Plugin(PluginNotice::InstalledRebootNeeded))
    );
    assert!(state
        .logs
        .iter()
        .any(|l| l.contains("Plugin installed in ur0:tai/")));

    // After a reboot the launcher answers.
    let booted = host(&dir, true);
    PluginService::refresh(&mut state, &booted);
    assert_eq!(state.plugin.status, PluginStatus::Enabled);

    PluginService::execute(&mut state, &repo, &booted, PluginRequest::SetEnabled(false));
    assert_eq!(state.plugin.status, PluginStatus::Disabled);
    assert!(!state.plugin.enabled);
    assert_eq!(toast(&state), Some(UiText::Plugin(PluginNotice::Disabled)));
    assert!(plugin_cfg(&dir).starts_with("enabled=0\n"));
    assert!(state.logs.iter().any(|l| l.contains("enabled=0")));

    PluginService::execute(&mut state, &repo, &booted, PluginRequest::SetEnabled(true));
    assert_eq!(state.plugin.status, PluginStatus::Enabled);
    assert_eq!(toast(&state), Some(UiText::Plugin(PluginNotice::Enabled)));

    // Reinstalling over a running launcher needs no reboot.
    PluginService::execute(&mut state, &repo, &booted, PluginRequest::Install);
    assert_eq!(toast(&state), Some(UiText::Plugin(PluginNotice::Installed)));

    PluginService::execute(&mut state, &repo, &booted, PluginRequest::Uninstall);
    assert_eq!(state.plugin.status, PluginStatus::NotInstalled);
    assert!(state.plugin.unload_pending());
    assert_eq!(
        toast(&state),
        Some(UiText::Plugin(PluginNotice::UninstalledRebootNeeded))
    );
    assert!(state.logs.iter().any(|l| l.contains("Plugin uninstalled.")));

    // Not loaded: no reboot to mention.
    PluginService::execute(&mut state, &repo, &before_boot, PluginRequest::Install);
    PluginService::execute(&mut state, &repo, &before_boot, PluginRequest::Uninstall);
    assert_eq!(
        toast(&state),
        Some(UiText::Plugin(PluginNotice::Uninstalled))
    );
    assert!(!state.plugin.unload_pending());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_toggle_of_a_stale_frame_is_ignored() {
    let (dir, repo, mut state) = setup("plugin-stale-toggle");
    seed(&dir, STOCK_CONFIG);
    let host = host(&dir, true);
    PluginService::refresh(&mut state, &host);
    PluginService::execute(&mut state, &repo, &host, PluginRequest::SetEnabled(false));
    assert_eq!(plugin_cfg(&dir), "", "nothing written while not installed");
    assert_eq!(state.toast_msg, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_failures_are_logged_and_shown() {
    let (dir, repo, mut state) = setup("plugin-failures");
    let host = host(&dir, false);
    PluginService::execute(&mut state, &repo, &host, PluginRequest::Install);
    assert_eq!(
        toast(&state),
        Some(UiText::Plugin(PluginNotice::BundleMissing))
    );
    assert!(state.logs.iter().any(
        |l| l.contains("Plugin: bundled module missing: app0:plugins/syncthing_launcher.suprx")
    ));

    seed(&dir, STOCK_CONFIG);
    std::fs::remove_file(common::plugin::local(&root(&dir), "ur0:tai/config.txt")).unwrap();
    PluginService::execute(&mut state, &repo, &host, PluginRequest::Install);
    assert_eq!(
        toast(&state),
        Some(UiText::Plugin(PluginNotice::TaiConfigMissing))
    );
    assert!(state
        .logs
        .iter()
        .any(|l| l.contains("Plugin: no taiHEN config.txt in ux0:tai/ nor ur0:tai/")));

    let err = PluginError::Io("ur0:tai/x".to_owned(), "denied".to_owned());
    assert_eq!(err.notice(), PluginNotice::WriteFailed);
    assert_eq!(err.log_line(), "Plugin: cannot write ur0:tai/x: denied");
    assert_eq!(state.plugin.status, PluginStatus::NotInstalled);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_unwritable_plugin_cfg_is_a_write_failure() {
    let (dir, repo, mut state) = setup("plugin-cfg-unwritable");
    seed(&dir, STOCK_CONFIG);
    let host = host(&dir, true);
    PluginService::install(&host).unwrap();
    PluginService::refresh(&mut state, &host);
    std::fs::remove_dir_all(format!("{dir}/data")).unwrap();
    std::fs::write(format!("{dir}/data"), "file").unwrap();
    PluginService::execute(&mut state, &repo, &host, PluginRequest::SetEnabled(false));
    assert_eq!(
        toast(&state),
        Some(UiText::Plugin(PluginNotice::WriteFailed))
    );
    assert!(matches!(
        PluginService::set_enabled(&host, true),
        Err(PluginError::Io(path, _)) if path == "plugin.cfg"
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
