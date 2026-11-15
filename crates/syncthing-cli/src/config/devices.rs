//! Device edits: adding, removing and looking up remote devices.

use syncthing_core::DeviceId;

use super::error::ConfigError;
use super::model::{
    default_addresses, is_device, parse_device_id, Config, DeviceConfig, DYNAMIC_ADDRESS,
};

impl Config {
    /// Adds a device. The name defaults to the 7-character short ID, the addresses to `dynamic`.
    pub fn add_device(
        &mut self,
        id: &str,
        name: Option<String>,
        addresses: Vec<String>,
    ) -> Result<DeviceConfig, ConfigError> {
        let device_id = parse_device_id(id)?;
        if self.device(&device_id).is_some() {
            return Err(ConfigError::DeviceExists(device_id.to_syncthing_string()));
        }
        let name = name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| device_id.short());
        let addresses = if addresses.is_empty() {
            default_addresses()
        } else {
            addresses.iter().map(|a| normalize_address(a)).collect()
        };
        let device = DeviceConfig {
            id: device_id.to_syncthing_string(),
            name,
            addresses,
        };
        self.devices.push(device.clone());
        Ok(device)
    }

    /// Removes a device and unshares it from every folder.
    pub fn remove_device(&mut self, id: &str) -> Result<(), ConfigError> {
        let device_id = parse_device_id(id)?;
        let index = self
            .devices
            .iter()
            .position(|d| d.device_id() == Some(device_id))
            .ok_or_else(|| ConfigError::UnknownDevice(device_id.to_syncthing_string()))?;
        self.devices.remove(index);
        for folder in &mut self.folders {
            folder.devices.retain(|entry| !is_device(entry, &device_id));
        }
        Ok(())
    }

    /// The configured device with this ID, if any.
    pub fn device(&self, id: &DeviceId) -> Option<&DeviceConfig> {
        self.devices
            .iter()
            .find(|d| d.device_id().as_ref() == Some(id))
    }
}

/// `host:port` becomes `tcp://host:port`; `dynamic` and URLs are kept.
fn normalize_address(address: &str) -> String {
    let address = address.trim();
    if address == DYNAMIC_ADDRESS || address.contains("://") {
        address.to_owned()
    } else {
        format!("tcp://{address}")
    }
}
