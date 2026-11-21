//! Serializable configuration model and device ID parsing.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use syncthing_core::{
    DeviceId, DEFAULT_ANNOUNCE_SERVERS, DEFAULT_LOOKUP_SERVER, DEFAULT_RELAY_POOL,
};

use super::error::ConfigError;

/// Device name used until the user changes it.
pub const DEFAULT_DEVICE_NAME: &str = "syncthing-cli";
/// Listen address used until the user changes it.
pub const DEFAULT_LISTEN: &str = "0.0.0.0:22000";
/// Interval between two scans of the local folders, in seconds.
pub const DEFAULT_RESCAN_INTERVAL_SECS: u64 = 60;

/// Address of a device whose address is discovered (LAN or Global Discovery).
pub const DYNAMIC_ADDRESS: &str = "dynamic";

/// The whole `config.toml` file. Missing fields take their defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub device_name: String,
    pub listen: String,
    pub local_discovery: bool,
    pub global_discovery: bool,
    pub relays: bool,
    /// Interval between two scans of the local folders (there is no filesystem watcher).
    pub rescan_interval_secs: u64,
    /// Global Discovery servers our addresses are announced to (`?id=` pins the certificate).
    pub global_announce_servers: Vec<String>,
    /// Global Discovery servers queried for the addresses of devices.
    pub global_lookup_servers: Vec<String>,
    /// Relay pool listing the public relays (`https://...`).
    pub relay_pool_url: String,
    /// Relays to join (`relay://host:port/?id=...`) instead of picking one from the pool.
    pub static_relays: Vec<String>,
    pub devices: Vec<DeviceConfig>,
    pub folders: Vec<FolderConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_name: DEFAULT_DEVICE_NAME.to_owned(),
            listen: DEFAULT_LISTEN.to_owned(),
            local_discovery: true,
            global_discovery: true,
            relays: true,
            rescan_interval_secs: DEFAULT_RESCAN_INTERVAL_SECS,
            global_announce_servers: DEFAULT_ANNOUNCE_SERVERS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            global_lookup_servers: vec![DEFAULT_LOOKUP_SERVER.to_owned()],
            relay_pool_url: DEFAULT_RELAY_POOL.to_owned(),
            static_relays: Vec::new(),
            devices: Vec::new(),
            folders: Vec::new(),
        }
    }
}

/// A remote device.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceConfig {
    /// Full device ID in canonical form (`to_syncthing_string()`).
    pub id: String,
    pub name: String,
    /// `dynamic` or `tcp://host:port`.
    #[serde(default = "default_addresses")]
    pub addresses: Vec<String>,
}

/// Synchronization mode of a folder in `config.toml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigFolderType {
    #[default]
    SendReceive,
    SendOnly,
    ReceiveOnly,
}

impl ConfigFolderType {
    /// Converts to the core BEP [`syncthing_core::FolderType`].
    pub fn to_bep(self) -> syncthing_core::FolderType {
        match self {
            Self::SendReceive => syncthing_core::FolderType::SendReceive,
            Self::SendOnly => syncthing_core::FolderType::SendOnly,
            Self::ReceiveOnly => syncthing_core::FolderType::ReceiveOnly,
        }
    }

    /// Lowercase kebab-case label (`send-receive`, `send-only`, `receive-only`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SendReceive => "send-receive",
            Self::SendOnly => "send-only",
            Self::ReceiveOnly => "receive-only",
        }
    }
}

/// A shared folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderConfig {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    /// Canonical device IDs this folder is shared with.
    pub devices: Vec<String>,
    /// Folder synchronization mode (`send-receive`, `send-only`, `receive-only`).
    pub folder_type: ConfigFolderType,
    /// Legacy alias for `folder_type = "receive-only"`.
    #[serde(skip_serializing_if = "is_false")]
    pub receive_only: bool,
    pub paused: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

pub(crate) fn default_addresses() -> Vec<String> {
    vec![DYNAMIC_ADDRESS.to_owned()]
}

impl DeviceConfig {
    /// The parsed device ID, or `None` when the stored text is not a full ID.
    pub fn device_id(&self) -> Option<DeviceId> {
        parse_device_id(&self.id).ok()
    }
}

impl FolderConfig {
    /// Effective folder type, honoring legacy `receive_only = true` when `folder_type` is default.
    pub fn effective_folder_type(&self) -> ConfigFolderType {
        if self.folder_type == ConfigFolderType::SendReceive && self.receive_only {
            ConfigFolderType::ReceiveOnly
        } else {
            self.folder_type
        }
    }

    /// Parsed IDs of the shared devices; invalid entries are skipped.
    pub fn device_ids(&self) -> Vec<DeviceId> {
        self.devices
            .iter()
            .filter_map(|entry| parse_device_id(entry).ok())
            .collect()
    }
}

/// Parses user input into a full device ID. Short IDs are rejected.
pub fn parse_device_id(input: &str) -> Result<DeviceId, ConfigError> {
    let trimmed = input.trim();
    let id = DeviceId::parse_user_id(trimmed)
        .map_err(|e| ConfigError::InvalidDeviceId(format!("`{trimmed}`: {e}")))?;
    if id.is_short_placeholder() {
        return Err(ConfigError::InvalidDeviceId(format!(
            "`{trimmed}`: a full device ID is required"
        )));
    }
    Ok(id)
}

/// `true` when the stored device entry designates `id`.
pub(crate) fn is_device(entry: &str, id: &DeviceId) -> bool {
    parse_device_id(entry).ok() == Some(*id)
}
