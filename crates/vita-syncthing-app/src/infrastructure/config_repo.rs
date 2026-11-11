//! Persistence and filesystem repository (`devices.cfg`, `shares.cfg`, `ignored_folders.cfg`,
//! `syncthing.log`, and `ux0:` directory browsing).

use std::fs::{self, OpenOptions};
use std::io::Write;

use syncthing_core::DeviceId;

use crate::domain::{IgnoredFolder, KnownDevice, ShareConfigEntry, NEW_SHARE_TEMPLATES};

/// Repository managing configuration persistence and local filesystem queries on `ux0:`.
#[derive(Debug, Clone)]
pub struct ConfigRepository {
    base_data_dir: String,
}

impl ConfigRepository {
    pub fn new(base_data_dir: &str) -> Self {
        Self {
            base_data_dir: base_data_dir.to_owned(),
        }
    }

    pub fn base_data_dir(&self) -> &str {
        &self.base_data_dir
    }

    #[cfg(target_os = "vita")]
    pub fn default_browse_root() -> &'static str {
        "ux0:"
    }

    #[cfg(not(target_os = "vita"))]
    pub fn default_browse_root() -> &'static str {
        "/tmp/syncthing-vita"
    }

    /// Appends a diagnostic log line to stdout and `<base_data_dir>/syncthing.log`.
    pub fn write_log(&self, msg: &str) {
        println!("[VITA-SYNCTHING] {}", msg);
        let _ = fs::create_dir_all(&self.base_data_dir);
        let log_path = format!("{}/syncthing.log", self.base_data_dir);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = writeln!(file, "[VITA-SYNCTHING] {}", msg);
        }
    }

    /// Ensures a directory path exists on disk.
    pub fn ensure_dir(&self, path: &str) {
        let _ = fs::create_dir_all(path);
    }

    /// Loads known remote devices from `<base_data_dir>/devices.cfg`.
    pub fn load_devices(&self) -> Vec<KnownDevice> {
        let cfg_path = format!("{}/devices.cfg", self.base_data_dir);
        let mut out = Vec::new();
        if let Ok(content) = fs::read_to_string(&cfg_path) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    if let Some(device_id) = DeviceId::from_hex(parts[0]) {
                        out.push(KnownDevice {
                            device_id,
                            name: parts[1].to_owned(),
                            address: parts[2].to_owned(),
                        });
                    }
                }
            }
        }
        out
    }

    /// Saves known remote devices to `<base_data_dir>/devices.cfg`.
    pub fn save_devices(&self, devices: &[KnownDevice]) {
        let _ = fs::create_dir_all(&self.base_data_dir);
        let cfg_path = format!("{}/devices.cfg", self.base_data_dir);
        let mut out = String::new();
        for d in devices {
            out.push_str(&format!(
                "{}|{}|{}\n",
                d.device_id.to_hex(),
                d.name,
                d.address
            ));
        }
        write_replacing(&cfg_path, &out);
    }

    /// Loads shared folder configuration from `<base_data_dir>/shares.cfg`.
    pub fn load_shares(&self) -> Vec<ShareConfigEntry> {
        let cfg_path = format!("{}/shares.cfg", self.base_data_dir);
        let mut entries = Vec::new();
        let mut needs_save = false;
        if let Ok(content) = fs::read_to_string(&cfg_path) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4 {
                    let shared_devices = if parts.len() >= 5 && !parts[4].trim().is_empty() {
                        parts[4]
                            .split(',')
                            .map(|s| s.trim().to_owned())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let folder_id = parts[0].to_owned();
                    let mut target_path = parts[2].to_owned();
                    let default_sync_path = format!("{}/sync/{}", self.base_data_dir, folder_id);
                    if target_path == default_sync_path {
                        if let Some(&(_, _, tpl_path)) = NEW_SHARE_TEMPLATES
                            .iter()
                            .find(|&&(id, _, _)| id == folder_id && id != "__browse__")
                        {
                            target_path = tpl_path.to_owned();
                            needs_save = true;
                        }
                    }
                    entries.push(ShareConfigEntry {
                        folder_id,
                        label: parts[1].to_owned(),
                        target_path,
                        enabled: parts[3] != "0",
                        shared_devices,
                    });
                }
            }
        }
        if needs_save {
            self.save_shares(&entries);
        }
        entries
    }

    /// Saves shared folder configuration to `<base_data_dir>/shares.cfg`.
    pub fn save_shares(&self, shares: &[ShareConfigEntry]) {
        let _ = fs::create_dir_all(&self.base_data_dir);
        let cfg_path = format!("{}/shares.cfg", self.base_data_dir);
        let mut out = String::new();
        for s in shares {
            let devs = s.shared_devices.join(",");
            out.push_str(&format!(
                "{}|{}|{}|{}|{}\n",
                s.folder_id,
                s.label,
                s.target_path,
                if s.enabled { "1" } else { "0" },
                devs
            ));
        }
        write_replacing(&cfg_path, &out);
    }

    /// Loads declined folder offers from `<base_data_dir>/ignored_folders.cfg`.
    pub fn load_ignored_folders(&self) -> Vec<IgnoredFolder> {
        let cfg_path = format!("{}/ignored_folders.cfg", self.base_data_dir);
        let Ok(content) = fs::read_to_string(&cfg_path) else {
            return Vec::new();
        };
        content
            .lines()
            .filter_map(|line| {
                let (folder_id, dev_hex) = line.split_once('|')?;
                Some(IgnoredFolder {
                    folder_id: folder_id.to_owned(),
                    device_id: DeviceId::from_hex(dev_hex.trim())?,
                })
            })
            .collect()
    }

    /// Saves declined folder offers to `<base_data_dir>/ignored_folders.cfg`.
    pub fn save_ignored_folders(&self, ignored: &[IgnoredFolder]) {
        let _ = fs::create_dir_all(&self.base_data_dir);
        let cfg_path = format!("{}/ignored_folders.cfg", self.base_data_dir);
        let out: String = ignored
            .iter()
            .map(|f| format!("{}|{}\n", f.folder_id, f.device_id.to_hex()))
            .collect();
        write_replacing(&cfg_path, &out);
    }

    /// Lists sorted non-hidden subdirectory names inside `dir_path`.
    pub fn list_subdirectories(&self, dir_path: &str) -> Vec<String> {
        let scan_dir = if dir_path == "ux0:" {
            "ux0:/"
        } else {
            dir_path
        };

        let mut subdirs = Vec::new();
        if let Ok(read_dir) = fs::read_dir(scan_dir) {
            for entry in read_dir.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if !name.starts_with('.') {
                            subdirs.push(name);
                        }
                    }
                }
            }
            subdirs.sort();
        }
        subdirs
    }
}

/// Replaces `path` with `content` via a fresh temp file + rename.
/// `sceIo` does not reliably honor `O_TRUNC`, so a shorter rewrite would keep the old file's tail.
fn write_replacing(path: &str, content: &str) {
    let tmp = format!("{}.tmp", path);
    let _ = fs::remove_file(&tmp);
    if fs::write(&tmp, content).is_ok() {
        let _ = fs::remove_file(path);
        let _ = fs::rename(&tmp, path);
    }
}
