//! Folder edits: adding, removing, sharing, pausing.

use std::path::PathBuf;

use super::error::ConfigError;
use super::model::{is_device, parse_device_id, Config, ConfigFolderType, FolderConfig};

impl Config {
    /// Adds a folder. Every device must already be configured; the label defaults to the ID.
    pub fn add_folder(
        &mut self,
        id: &str,
        path: PathBuf,
        label: Option<String>,
        devices: Vec<String>,
        receive_only: bool,
    ) -> Result<(), ConfigError> {
        let folder_type = if receive_only {
            ConfigFolderType::ReceiveOnly
        } else {
            ConfigFolderType::SendReceive
        };
        self.add_folder_with_type(id, path, label, devices, folder_type)
    }

    /// Adds a folder with an explicit [`ConfigFolderType`].
    pub fn add_folder_with_type(
        &mut self,
        id: &str,
        path: PathBuf,
        label: Option<String>,
        devices: Vec<String>,
        folder_type: ConfigFolderType,
    ) -> Result<(), ConfigError> {
        let id = id.trim();
        if id.is_empty() {
            return Err(invalid_value("folder ID", id));
        }
        if self.folders.iter().any(|f| f.id == id) {
            return Err(ConfigError::FolderExists(id.to_owned()));
        }
        let mut shared: Vec<String> = Vec::new();
        for device in &devices {
            let device_id = parse_device_id(device)?;
            if self.device(&device_id).is_none() {
                return Err(ConfigError::UnknownDevice(device_id.to_syncthing_string()));
            }
            let canonical = device_id.to_syncthing_string();
            if !shared.contains(&canonical) {
                shared.push(canonical);
            }
        }
        let label = label
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| id.to_owned());
        self.folders.push(FolderConfig {
            id: id.to_owned(),
            label,
            path,
            devices: shared,
            folder_type,
            receive_only: folder_type == ConfigFolderType::ReceiveOnly,
            paused: false,
            versioning: super::model::FolderVersioningConfig::None,
        });
        Ok(())
    }

    /// Removes a folder from the configuration (its files are kept).
    pub fn remove_folder(&mut self, id: &str) -> Result<(), ConfigError> {
        let index = self.folder_index(id)?;
        self.folders.remove(index);
        Ok(())
    }

    /// Shares a folder with a configured device. Sharing twice is a no-op.
    pub fn share_folder(&mut self, folder: &str, device: &str) -> Result<(), ConfigError> {
        let device_id = parse_device_id(device)?;
        if self.device(&device_id).is_none() {
            return Err(ConfigError::UnknownDevice(device_id.to_syncthing_string()));
        }
        let index = self.folder_index(folder)?;
        let entry = &mut self.folders[index];
        if !entry.device_ids().contains(&device_id) {
            entry.devices.push(device_id.to_syncthing_string());
        }
        Ok(())
    }

    /// Stops sharing a folder with a device. Unsharing an unshared device is a no-op.
    pub fn unshare_folder(&mut self, folder: &str, device: &str) -> Result<(), ConfigError> {
        let device_id = parse_device_id(device)?;
        let index = self.folder_index(folder)?;
        self.folders[index]
            .devices
            .retain(|entry| !is_device(entry, &device_id));
        Ok(())
    }

    /// Pauses or resumes a folder.
    pub fn set_folder_paused(&mut self, folder: &str, paused: bool) -> Result<(), ConfigError> {
        let index = self.folder_index(folder)?;
        self.folders[index].paused = paused;
        Ok(())
    }

    fn folder_index(&self, id: &str) -> Result<usize, ConfigError> {
        let id = id.trim();
        self.folders
            .iter()
            .position(|f| f.id == id)
            .ok_or_else(|| ConfigError::UnknownFolder(id.to_owned()))
    }
}

fn invalid_value(key: &str, value: &str) -> ConfigError {
    ConfigError::InvalidValue {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}
