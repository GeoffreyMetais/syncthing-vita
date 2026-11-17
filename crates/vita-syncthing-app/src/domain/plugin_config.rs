//! `ux0:data/syncthing/plugin.cfg`: settings of the background sync plugin, read by its
//! launcher (`syncthing_launcher.suprx`) before each sync cycle and written by `Settings`.
//!
//! Plain `key=value` lines (LF): `enabled=0|1`, `sync_on_game_exit=0|1`,
//! `interval_secs=<u32>`. A missing or invalid value takes its default; lines this version does
//! not know (other keys, comments) are kept when the file is rewritten.

use crate::domain::defaults::{
    PLUGIN_DEFAULT_ENABLED, PLUGIN_DEFAULT_INTERVAL_SECS, PLUGIN_DEFAULT_SYNC_ON_GAME_EXIT,
};

const KEY_ENABLED: &str = "enabled";
const KEY_SYNC_ON_GAME_EXIT: &str = "sync_on_game_exit";
const KEY_INTERVAL_SECS: &str = "interval_secs";

/// Contents of `plugin.cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginConfig {
    /// Background sync allowed (the launcher stays idle otherwise).
    pub enabled: bool,
    /// Sync right after a game exits.
    pub sync_on_game_exit: bool,
    /// Interval between background syncs while no game runs, in seconds.
    pub interval_secs: u32,
    /// Lines with another key, or comments, kept verbatim and in order.
    other_lines: Vec<String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: PLUGIN_DEFAULT_ENABLED,
            sync_on_game_exit: PLUGIN_DEFAULT_SYNC_ON_GAME_EXIT,
            interval_secs: PLUGIN_DEFAULT_INTERVAL_SECS,
            other_lines: Vec::new(),
        }
    }
}

/// `0` / `1` flag.
fn parse_flag(value: &str) -> Option<bool> {
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

impl PluginConfig {
    /// Parses `plugin.cfg` (LF or CRLF). Missing or invalid known values keep their default;
    /// when a key appears twice, its last valid value wins.
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                cfg.other_lines.push(line.to_owned());
                continue;
            };
            match key.trim() {
                KEY_ENABLED => cfg.enabled = parse_flag(value).unwrap_or(cfg.enabled),
                KEY_SYNC_ON_GAME_EXIT => {
                    cfg.sync_on_game_exit = parse_flag(value).unwrap_or(cfg.sync_on_game_exit)
                }
                KEY_INTERVAL_SECS => {
                    cfg.interval_secs = value.trim().parse().unwrap_or(cfg.interval_secs)
                }
                _ => cfg.other_lines.push(line.to_owned()),
            }
        }
        cfg
    }

    /// Lines kept from the parsed file that this version does not interpret.
    pub fn other_lines(&self) -> &[String] {
        &self.other_lines
    }

    /// File contents: the three known keys first (always written), then the kept lines; LF.
    pub fn serialize(&self) -> String {
        let mut out = format!(
            "{KEY_ENABLED}={}\n{KEY_SYNC_ON_GAME_EXIT}={}\n{KEY_INTERVAL_SECS}={}\n",
            u8::from(self.enabled),
            u8::from(self.sync_on_game_exit),
            self.interval_secs
        );
        for line in &self.other_lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}
