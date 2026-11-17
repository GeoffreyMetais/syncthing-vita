//! `plugin.cfg` as the worker reads it (the launcher's C parser follows the same rules:
//! `launcher_policy.rs`).

mod common;

use syncthing_vita_worker::plugin_cfg::{
    DEFAULT_ENABLED, DEFAULT_INTERVAL_SECS, DEFAULT_SYNC_ON_GAME_EXIT,
};
use syncthing_vita_worker::PluginSettings;

fn parse(text: &str) -> PluginSettings {
    PluginSettings::parse(text)
}

#[test]
fn test_defaults() {
    const { assert!(DEFAULT_ENABLED) };
    const { assert!(DEFAULT_SYNC_ON_GAME_EXIT) };
    assert_eq!(DEFAULT_INTERVAL_SECS, 1800);
    assert_eq!(
        parse(""),
        PluginSettings {
            enabled: true,
            sync_on_game_exit: true,
            interval_secs: 1800
        }
    );
}

#[test]
fn test_values_are_read() {
    assert_eq!(
        parse("enabled=0\nsync_on_game_exit=0\ninterval_secs=600\n"),
        PluginSettings {
            enabled: false,
            sync_on_game_exit: false,
            interval_secs: 600
        }
    );
    assert_eq!(
        parse("enabled=1\r\nsync_on_game_exit=1\r\ninterval_secs=0\r\n"),
        PluginSettings {
            enabled: true,
            sync_on_game_exit: true,
            interval_secs: 0
        }
    );
}

#[test]
fn test_spaces_invalid_values_and_repeated_keys() {
    let s = parse("  enabled = 0  \ninterval_secs=abc\nsync_on_game_exit=yes\n");
    assert!(!s.enabled);
    assert_eq!(s.interval_secs, 1800, "invalid value keeps the default");
    assert!(s.sync_on_game_exit, "only 0 / 1 are flags");
    let s = parse("interval_secs=60\ninterval_secs=-5\ninterval_secs=90\nenabled=1\nenabled=0");
    assert_eq!(s.interval_secs, 90, "the last valid value wins");
    assert!(!s.enabled);
    let s = parse("# comment\nunknown=1\nenabled\n=0\ninterval_secs=4294967296\n");
    assert_eq!(s, PluginSettings::DEFAULTS);
    assert_eq!(parse("interval_secs=4294967295").interval_secs, u32::MAX);
    assert_eq!(parse("interval_secs=+7").interval_secs, 7);
    let s = parse("enabled=0\nsync_on_game_exit=0\nenabled=1\nsync_on_game_exit=1");
    assert!(s.enabled && s.sync_on_game_exit, "1 turns a flag back on");
}

#[test]
fn test_missing_file_is_none_and_backup_is_used() {
    let dir = common::fresh_temp_dir("plugin_cfg_load");
    assert_eq!(PluginSettings::load(&dir), None);
    std::fs::write(dir.join("plugin.cfg.bak"), "enabled=0\n").unwrap();
    assert_eq!(PluginSettings::load(&dir).map(|s| s.enabled), Some(false));
    std::fs::write(dir.join("plugin.cfg"), "enabled=1\ninterval_secs=42\n").unwrap();
    assert_eq!(
        PluginSettings::load(&dir),
        Some(PluginSettings {
            enabled: true,
            sync_on_game_exit: true,
            interval_secs: 42
        })
    );
}

#[test]
fn test_reads_what_the_app_writes_and_parses_like_the_app() {
    use vita_syncthing_app::domain::PluginConfig;
    let inputs = [
        "",
        "enabled=0\n",
        "enabled=1\nsync_on_game_exit=0\ninterval_secs=600\n",
        "  enabled = 0  \r\ninterval_secs = 3600\r\n",
        "interval_secs=abc\nsync_on_game_exit=yes\n",
        "interval_secs=60\ninterval_secs=-5\ninterval_secs=90\n",
        "# kept\nother=1\nenabled\n=0\n",
    ];
    for text in inputs {
        let app = PluginConfig::parse(text);
        let ours = parse(text);
        assert_eq!(ours.enabled, app.enabled, "{text:?}");
        assert_eq!(ours.sync_on_game_exit, app.sync_on_game_exit, "{text:?}");
        assert_eq!(ours.interval_secs, app.interval_secs, "{text:?}");
        // What the app writes back reads the same.
        assert_eq!(parse(&app.serialize()), ours, "{text:?}");
    }
}
