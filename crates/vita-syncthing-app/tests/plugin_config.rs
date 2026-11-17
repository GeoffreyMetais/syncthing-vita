//! `plugin.cfg` codec (`domain::PluginConfig`): defaults, `0` / `1` flags, interval, unknown
//! lines kept, invalid values ignored, exact serialized form.

use vita_syncthing_app::domain::defaults::{
    PLUGIN_DEFAULT_ENABLED, PLUGIN_DEFAULT_INTERVAL_SECS, PLUGIN_DEFAULT_SYNC_ON_GAME_EXIT,
};
use vita_syncthing_app::domain::PluginConfig;

#[test]
fn test_defaults() {
    assert_eq!(PLUGIN_DEFAULT_INTERVAL_SECS, 1800);
    let cfg = PluginConfig::default();
    assert!(cfg.enabled);
    assert!(cfg.sync_on_game_exit);
    assert_eq!(cfg.enabled, PLUGIN_DEFAULT_ENABLED);
    assert_eq!(cfg.sync_on_game_exit, PLUGIN_DEFAULT_SYNC_ON_GAME_EXIT);
    assert_eq!(cfg.interval_secs, 1800);
    assert!(cfg.other_lines().is_empty());
    assert_eq!(PluginConfig::parse(""), cfg);
}

#[test]
fn test_serialized_form() {
    assert_eq!(
        PluginConfig::default().serialize(),
        "enabled=1\nsync_on_game_exit=1\ninterval_secs=1800\n"
    );
    let mut cfg = PluginConfig::parse("enabled=0\nsync_on_game_exit=0\ninterval_secs=60\n");
    assert!(!cfg.enabled);
    assert!(!cfg.sync_on_game_exit);
    assert_eq!(cfg.interval_secs, 60);
    assert_eq!(
        cfg.serialize(),
        "enabled=0\nsync_on_game_exit=0\ninterval_secs=60\n"
    );
    cfg.enabled = true;
    assert_eq!(
        cfg.serialize(),
        "enabled=1\nsync_on_game_exit=0\ninterval_secs=60\n"
    );
}

#[test]
fn test_missing_keys_take_defaults() {
    let cfg = PluginConfig::parse("enabled=0\n");
    assert!(!cfg.enabled);
    assert!(cfg.sync_on_game_exit);
    assert_eq!(cfg.interval_secs, 1800);
    let cfg = PluginConfig::parse("sync_on_game_exit=0");
    assert!(cfg.enabled);
    assert!(!cfg.sync_on_game_exit);
}

#[test]
fn test_invalid_values_keep_the_default_and_are_not_kept() {
    let cfg =
        PluginConfig::parse("enabled=yes\nsync_on_game_exit=2\ninterval_secs=-5\ninterval_secs=\n");
    assert_eq!(cfg, PluginConfig::default());
    assert_eq!(
        cfg.serialize(),
        "enabled=1\nsync_on_game_exit=1\ninterval_secs=1800\n"
    );
    let cfg = PluginConfig::parse("enabled=0\nenabled=maybe\n");
    assert!(
        !cfg.enabled,
        "an invalid repeat keeps the previous valid value"
    );
}

#[test]
fn test_last_valid_value_wins_and_spaces_are_trimmed() {
    let cfg = PluginConfig::parse(
        "enabled=0\n  enabled = 1  \ninterval_secs=10\ninterval_secs = 4294967295\n",
    );
    assert!(cfg.enabled);
    assert_eq!(cfg.interval_secs, u32::MAX);
    let cfg = PluginConfig::parse("interval_secs=4294967296\n");
    assert_eq!(cfg.interval_secs, 1800, "out of range");
}

#[test]
fn test_crlf_is_read() {
    let cfg = PluginConfig::parse("enabled=0\r\nsync_on_game_exit=0\r\ninterval_secs=90\r\n");
    assert!(!cfg.enabled);
    assert!(!cfg.sync_on_game_exit);
    assert_eq!(cfg.interval_secs, 90);
    assert!(cfg.other_lines().is_empty());
}

#[test]
fn test_unknown_lines_are_kept_in_order() {
    let text = "# written by the app\nenabled=0\nlog_level=2\n\nfuture flag\nwifi_only = 1\r\n";
    let cfg = PluginConfig::parse(text);
    assert_eq!(
        cfg.other_lines(),
        [
            "# written by the app",
            "log_level=2",
            "future flag",
            "wifi_only = 1"
        ]
    );
    assert_eq!(
        cfg.serialize(),
        "enabled=0\nsync_on_game_exit=1\ninterval_secs=1800\n# written by the app\nlog_level=2\nfuture flag\nwifi_only = 1\n"
    );
    assert_eq!(PluginConfig::parse(&cfg.serialize()), cfg);
}
