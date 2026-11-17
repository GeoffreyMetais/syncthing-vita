//! `PluginHost`: device paths mapped below a root on host builds, crash-safe writes, removal,
//! `plugin.cfg` persistence (defaults, kept keys, interrupted replacement) and the probe.

mod common;

use common::plugin::{host, local, put, root, text};
use common::temp_test_dir;
use vita_syncthing_app::domain::PluginConfig;
use vita_syncthing_app::infrastructure::{
    FixedLauncherProbe, LauncherProbe, PluginHost, PLUGIN_CFG_FILE,
};

#[test]
fn test_device_paths_are_mapped_below_the_root() {
    let host = PluginHost::rooted("/r/", "/d", Box::new(FixedLauncherProbe(false)));
    assert_eq!(
        host.local_path("ux0:tai/config.txt"),
        "/r/ux0/tai/config.txt"
    );
    assert_eq!(host.local_path("ur0:/tai/x"), "/r/ur0/tai/x");
    assert_eq!(
        host.local_path("app0:plugins/a.suprx"),
        "/r/app0/plugins/a.suprx"
    );
    assert_eq!(host.local_path("/plain/file"), "/r/plain/file");
    assert_eq!(PLUGIN_CFG_FILE, "plugin.cfg");
}

#[test]
fn test_system_host_of_host_builds() {
    let dir = temp_test_dir("plugin-host-system");
    let host = PluginHost::system(&dir);
    assert_eq!(
        host.local_path("ux0:tai/config.txt"),
        format!("{dir}/devices/ux0/tai/config.txt")
    );
    assert!(!host.launcher_alive());
    host.save_plugin_config(&PluginConfig::default()).unwrap();
    assert!(std::path::Path::new(&format!("{dir}/plugin.cfg")).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_probe_answers() {
    assert!(FixedLauncherProbe(true).launcher_alive());
    assert!(!FixedLauncherProbe(false).launcher_alive());
    let dir = temp_test_dir("plugin-host-probe");
    assert!(host(&dir, true).launcher_alive());
    assert!(!host(&dir, false).launcher_alive());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_write_read_exists_remove() {
    let dir = temp_test_dir("plugin-host-files");
    let host = host(&dir, false);
    let path = "ur0:tai/new/module.suprx";
    assert!(!host.exists(path));
    assert!(host.read(path).is_err());
    assert_eq!(host.read_text(path), None);
    // Parent directories are created.
    host.write(path, b"one").unwrap();
    assert!(host.exists(path));
    assert_eq!(host.read(path).unwrap(), b"one");
    host.write(path, b"two").unwrap();
    assert_eq!(host.read_text(path).as_deref(), Some("two"));
    // No temp file or transient backup is left behind.
    let parent = format!("{}/ur0/tai/new", root(&dir));
    let names: Vec<_> = std::fs::read_dir(&parent)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["module.suprx"]);
    host.remove(path).unwrap();
    assert!(!host.exists(path));
    // Removing a missing file is not an error; removing a directory is.
    host.remove(path).unwrap();
    assert!(host.remove("ur0:tai/new").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_non_utf8_text_reads_as_none() {
    let dir = temp_test_dir("plugin-host-binary");
    put(&root(&dir), "ur0:tai/config.txt", &[0xff, 0xfe, 0x00]);
    assert_eq!(host(&dir, false).read_text("ur0:tai/config.txt"), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_interrupted_tai_replacement_is_recovered() {
    let dir = temp_test_dir("plugin-host-recover");
    let root = root(&dir);
    // The previous version survived as `.old`, the new one never landed.
    put(&root, "ur0:tai/config.txt.old", b"*main\n");
    let host = host(&dir, false);
    assert!(host.exists("ur0:tai/config.txt"));
    assert_eq!(
        host.read_text("ur0:tai/config.txt").as_deref(),
        Some("*main\n")
    );
    assert!(!std::path::Path::new(&local(&root, "ur0:tai/config.txt.old")).exists());

    put(&root, "ur0:tai/a.suprx.old", b"old");
    assert_eq!(host.read("ur0:tai/a.suprx").unwrap(), b"old");
    // `remove` also drops a leftover backup.
    put(&root, "ur0:tai/b.suprx", b"b");
    put(&root, "ur0:tai/b.suprx.old", b"b-old");
    host.remove("ur0:tai/b.suprx").unwrap();
    assert!(!host.exists("ur0:tai/b.suprx"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_plugin_cfg_defaults_save_and_reload() {
    let dir = temp_test_dir("plugin-host-cfg");
    let host = host(&dir, false);
    assert_eq!(host.load_plugin_config(), PluginConfig::default());
    let mut cfg = PluginConfig::parse("custom=7\n");
    cfg.enabled = false;
    cfg.interval_secs = 600;
    // The data directory is created.
    host.save_plugin_config(&cfg).unwrap();
    let path = format!("{dir}/data/plugin.cfg");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "enabled=0\nsync_on_game_exit=1\ninterval_secs=600\ncustom=7\n"
    );
    assert_eq!(host.load_plugin_config(), cfg);
    assert!(!std::path::Path::new(&format!("{path}.bak")).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_interrupted_plugin_cfg_replacement_is_recovered() {
    let dir = temp_test_dir("plugin-host-cfg-recover");
    std::fs::create_dir_all(format!("{dir}/data")).unwrap();
    std::fs::write(format!("{dir}/data/plugin.cfg.bak"), "enabled=0\n").unwrap();
    let host = host(&dir, false);
    assert!(!host.load_plugin_config().enabled);
    assert!(std::path::Path::new(&format!("{dir}/data/plugin.cfg")).exists());
    assert_eq!(text(&root(&dir), "ux0:tai/config.txt"), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_writing_below_a_file_fails() {
    let dir = temp_test_dir("plugin-host-fail");
    let root = root(&dir);
    put(&root, "ur0:tai", b"not a directory");
    let host = host(&dir, false);
    assert!(host.write("ur0:tai/x.suprx", b"x").is_err());
    // A data directory that is a file cannot hold `plugin.cfg`.
    std::fs::write(format!("{dir}/data"), "file").unwrap();
    assert!(host.save_plugin_config(&PluginConfig::default()).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
