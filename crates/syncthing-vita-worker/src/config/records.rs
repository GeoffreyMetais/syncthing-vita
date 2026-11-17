//! Records of the app's `devices.cfg`, `shares.cfg` and `ignored_folders.cfg`, parsed exactly
//! as the app's `ConfigRepository` loads them (formats in AGENTS.md §5):
//!
//! - `devices.cfg`: `<device_id_hex>|<name>|<ip:port>`
//! - `shares.cfg`: `<folder_id>|<label>|<target_path>|<0|1>|<dev_hex1,dev_hex2>|<rw|ro>`
//! - `ignored_folders.cfg`: `<folder_id>|<device_id_hex>`
//!
//! The worker only reads them: it never rewrites the app's configuration.

use syncthing_core::{DeviceId, FolderType};

use super::fields::unescape_field;

/// A device of `devices.cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub name: String,
    /// `ip:port`, `hostname:port` or `dynamic`.
    pub address: String,
}

/// A share of `shares.cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRecord {
    pub folder_id: String,
    pub label: String,
    pub target_path: String,
    /// `false`: the share is paused.
    pub enabled: bool,
    /// The devices the folder is shared with (unparseable entries dropped, as the app does).
    pub devices: Vec<DeviceId>,
    /// `ro` is a read-only (Syncthing "Receive Only") share, anything else read & write.
    pub read_only: bool,
}

impl ShareRecord {
    /// Folder type announced and applied by the sessions.
    pub fn folder_type(&self) -> FolderType {
        if self.read_only {
            FolderType::ReceiveOnly
        } else {
            FolderType::SendReceive
        }
    }

    /// `true` if `device` is one of the share's devices (short ID placeholders included).
    pub fn is_linked_to(&self, device: &DeviceId) -> bool {
        self.devices.iter().any(|id| id.matches_peer(device))
    }
}

/// A declined folder offer of `ignored_folders.cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredRecord {
    pub folder_id: String,
    pub device_id: DeviceId,
}

/// Parses `devices.cfg`. Lines with fewer than 3 fields or an invalid device ID are skipped.
pub fn parse_devices(text: &str) -> Vec<DeviceRecord> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 3 {
                return None;
            }
            Some(DeviceRecord {
                device_id: DeviceId::from_hex(parts[0])?,
                name: unescape_field(parts[1]),
                address: unescape_field(parts[2]),
            })
        })
        .collect()
}

/// Parses `shares.cfg`. Lines with fewer than 4 fields are skipped; a missing device list
/// means no device, a missing mode means read & write.
pub fn parse_shares(text: &str) -> Vec<ShareRecord> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 4 {
                return None;
            }
            let devices = match parts.get(4) {
                Some(list) if !list.trim().is_empty() => list
                    .split(',')
                    .filter_map(|hex| DeviceId::from_hex(hex.trim()))
                    .collect(),
                _ => Vec::new(),
            };
            Some(ShareRecord {
                folder_id: unescape_field(parts[0]),
                label: unescape_field(parts[1]),
                target_path: unescape_field(parts[2]),
                enabled: parts[3] != "0",
                devices,
                read_only: parts.get(5).is_some_and(|mode| mode.trim() == "ro"),
            })
        })
        .collect()
}

/// Parses `ignored_folders.cfg`. Lines without `|` or with an invalid device ID are skipped.
pub fn parse_ignored(text: &str) -> Vec<IgnoredRecord> {
    text.lines()
        .filter_map(|line| {
            let (folder_id, dev_hex) = line.split_once('|')?;
            Some(IgnoredRecord {
                folder_id: unescape_field(folder_id),
                device_id: DeviceId::from_hex(dev_hex.trim())?,
            })
        })
        .collect()
}
