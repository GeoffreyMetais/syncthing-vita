//! `plugin.cfg`, as the plugin reads it: `key=value` lines written by the app's `Settings`
//! (`enabled=0|1`, `sync_on_game_exit=0|1`, `interval_secs=<u32>`).
//!
//! The launcher (`plugin/launcher/src/policy.c`) parses the same file with the same rules,
//! checked by `tests/launcher_policy.rs`; the worker reads it again before its cycle so a
//! toggle made between the launcher's decision and the cycle is honoured.
//!
//! Rules: lines are trimmed (LF or CRLF), unknown keys and lines without `=` are ignored,
//! an invalid value keeps the previous one, the last valid value of a key wins. A key missing
//! from the file takes the app's default (`enabled=1`: the app writes the file when it
//! installs the plugin), but a missing file means "idle": nothing was installed or configured.

use std::fs;
use std::path::Path;

/// Name of the file in the app's data directory.
pub const PLUGIN_CFG_FILE: &str = "plugin.cfg";
/// `enabled` when the key is missing from an existing file (the app's default).
pub const DEFAULT_ENABLED: bool = true;
/// `sync_on_game_exit` when the key is missing.
pub const DEFAULT_SYNC_ON_GAME_EXIT: bool = true;
/// `interval_secs` when the key is missing (30 minutes).
pub const DEFAULT_INTERVAL_SECS: u32 = 1800;

/// Settings of the background sync plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginSettings {
    pub enabled: bool,
    pub sync_on_game_exit: bool,
    /// Seconds between two cycles while nothing runs; `0` disables the periodic cycles.
    pub interval_secs: u32,
}

impl PluginSettings {
    /// Settings of a file without any known key.
    pub const DEFAULTS: Self = Self {
        enabled: DEFAULT_ENABLED,
        sync_on_game_exit: DEFAULT_SYNC_ON_GAME_EXIT,
        interval_secs: DEFAULT_INTERVAL_SECS,
    };

    /// Parses the content of `plugin.cfg`.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::DEFAULTS;
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "enabled" => settings.enabled = parse_flag(value).unwrap_or(settings.enabled),
                "sync_on_game_exit" => {
                    settings.sync_on_game_exit =
                        parse_flag(value).unwrap_or(settings.sync_on_game_exit)
                }
                "interval_secs" => {
                    settings.interval_secs = value.parse().unwrap_or(settings.interval_secs)
                }
                _ => {}
            }
        }
        settings
    }

    /// Reads `<data_dir>/plugin.cfg`. A missing or unreadable file gives `None` (idle).
    pub fn load(data_dir: &Path) -> Option<Self> {
        let path = data_dir.join(PLUGIN_CFG_FILE);
        let backup = data_dir.join(format!("{PLUGIN_CFG_FILE}.bak"));
        // Read only: the app restores an interrupted replacement itself.
        fs::read_to_string(&path)
            .or_else(|_| fs::read_to_string(&backup))
            .ok()
            .map(|text| Self::parse(&text))
    }
}

fn parse_flag(value: &str) -> Option<bool> {
    match value {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}
