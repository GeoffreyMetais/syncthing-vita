//! Persistence and filesystem repository (`devices.cfg`, `shares.cfg`, `ignored_folders.cfg`,
//! `theme.cfg`, `language.cfg`, `syncthing.log`, and `ux0:` directory browsing).

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use syncthing_core::storage::atomic::{recover_replaced_file, replace_file, write_file_synced};
use syncthing_core::DeviceId;

use super::config_fields::{escape_field, unescape_field};
use crate::domain::defaults::LOG_MAX_BYTES;
use crate::domain::{
    IgnoredFolder, KnownDevice, LanguagePreference, ShareConfigEntry, ShareMode, ThemeMode,
    NEW_SHARE_TEMPLATES,
};

/// Prefix of every line of `syncthing.log`.
const LOG_LINE_PREFIX: &str = "[VITA-SYNCTHING] ";

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

    /// Default directory of a share: `<base_data_dir>/sync/<folder_id>`. Folder IDs come from
    /// remote peers, so only `A-Z a-z 0-9 - _` are kept: an ID such as `../..` can never point
    /// the share outside the `sync` directory.
    pub fn default_share_dir(&self, folder_id: &str) -> String {
        let clean: String = folder_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let dir = if clean.is_empty() { "folder" } else { &clean };
        format!("{}/sync/{}", self.base_data_dir, dir)
    }

    fn log_path(&self) -> String {
        format!("{}/syncthing.log", self.base_data_dir)
    }

    /// Previous log file, kept when `syncthing.log` is rotated.
    fn rotated_log_path(&self) -> String {
        format!("{}/syncthing.log.1", self.base_data_dir)
    }

    /// Returns `true` if `path` must never be a share's folder: the app's data directory
    /// (identity seed, `.cfg` files) or a folder containing it (`ux0:`, `ux0:data`). A peer of
    /// such a share could read the device's private key or rewrite the configuration.
    /// Folders inside the data directory (`sync/<id>`, `shared`) hold none of that.
    pub fn is_protected_share_path(&self, path: &str) -> bool {
        let base = path_components(&self.base_data_dir);
        let target = path_components(path);
        target.len() <= base.len() && base.iter().zip(&target).all(|(a, b)| a == b)
    }

    /// Appends a diagnostic log line to stdout and `<base_data_dir>/syncthing.log`. Above
    /// `LOG_MAX_BYTES` the file becomes `syncthing.log.1` and a new one is started.
    pub fn write_log(&self, msg: &str) {
        println!("{LOG_LINE_PREFIX}{msg}");
        let log_path = self.log_path();
        let open = || OpenOptions::new().create(true).append(true).open(&log_path);
        let file = open().or_else(|_| {
            // First line of a fresh installation: the data directory does not exist yet.
            let _ = fs::create_dir_all(&self.base_data_dir);
            open()
        });
        let Ok(mut file) = file else {
            return;
        };
        let _ = writeln!(file, "{LOG_LINE_PREFIX}{msg}");
        let too_large = file.metadata().is_ok_and(|m| m.len() > LOG_MAX_BYTES);
        drop(file);
        if too_large {
            let rotated = self.rotated_log_path();
            let _ = fs::remove_file(&rotated);
            let _ = fs::rename(&log_path, &rotated);
        }
    }

    /// Reads the last `max_lines` lines of the log (`syncthing.log.1` then `syncthing.log`, oldest first,
    /// without their prefix). The file is streamed: only `max_lines` lines are held at once.
    pub fn read_log_history(&self, max_lines: usize) -> Vec<String> {
        let mut tail = VecDeque::with_capacity(max_lines.min(256));
        if max_lines == 0 {
            return Vec::new();
        }
        // The rotated file holds the older lines.
        for path in [self.rotated_log_path(), self.log_path()] {
            let Ok(file) = fs::File::open(&path) else {
                continue;
            };
            for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
                if tail.len() == max_lines {
                    tail.pop_front();
                }
                let text = String::from_utf8_lossy(&line);
                let text = text.strip_prefix(LOG_LINE_PREFIX).unwrap_or(&text);
                tail.push_back(text.to_owned());
            }
        }
        tail.into()
    }

    /// Ensures a directory path exists on disk.
    pub fn ensure_dir(&self, path: &str) {
        let _ = fs::create_dir_all(path);
    }

    /// Path of a `.cfg` file in the data directory.
    fn cfg_path(&self, name: &str) -> String {
        format!("{}/{}", self.base_data_dir, name)
    }

    /// Loads a `.cfg` file as rows of unescaped `|`-separated fields (a missing or unreadable
    /// file yields no rows).
    fn load_rows(&self, name: &str) -> Vec<Vec<String>> {
        let Ok(content) = read_recovering(&self.cfg_path(name)) else {
            return Vec::new();
        };
        content
            .lines()
            .map(|line| line.split('|').map(unescape_field).collect())
            .collect()
    }

    /// Saves rows of fields to a `.cfg` file, one `|`-separated escaped row per line, through
    /// the atomic temp-file replacement.
    fn save_rows(&self, name: &str, rows: &[Vec<String>]) {
        let _ = fs::create_dir_all(&self.base_data_dir);
        let mut out = String::new();
        for row in rows {
            for (i, field) in row.iter().enumerate() {
                if i > 0 {
                    out.push('|');
                }
                out.push_str(&escape_field(field));
            }
            out.push('\n');
        }
        write_replacing(&self.cfg_path(name), &out);
    }

    /// Loads known remote devices from `<base_data_dir>/devices.cfg`.
    pub fn load_devices(&self) -> Vec<KnownDevice> {
        self.load_rows("devices.cfg")
            .into_iter()
            .filter_map(|row| {
                if row.len() < 3 {
                    return None;
                }
                Some(KnownDevice {
                    device_id: DeviceId::from_hex(&row[0])?,
                    name: row[1].clone(),
                    address: row[2].clone(),
                })
            })
            .collect()
    }

    /// Saves known remote devices to `<base_data_dir>/devices.cfg`.
    pub fn save_devices(&self, devices: &[KnownDevice]) {
        let rows: Vec<Vec<String>> = devices
            .iter()
            .map(|d| vec![d.device_id.to_hex(), d.name.clone(), d.address.clone()])
            .collect();
        self.save_rows("devices.cfg", &rows);
    }

    /// Loads shared folder configuration from `<base_data_dir>/shares.cfg`.
    pub fn load_shares(&self) -> Vec<ShareConfigEntry> {
        let mut entries = Vec::new();
        let mut needs_save = false;
        for row in self.load_rows("shares.cfg") {
            if row.len() < 4 {
                continue;
            }
            let shared_devices = if row.len() >= 5 && !row[4].trim().is_empty() {
                row[4]
                    .split(',')
                    .filter_map(|hex| DeviceId::from_hex(hex.trim()))
                    .collect()
            } else {
                Vec::new()
            };
            let folder_id = row[0].clone();
            let mut target_path = row[2].clone();
            let default_sync_path = self.default_share_dir(&folder_id);
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
                label: row[1].clone(),
                target_path,
                enabled: row[3] != "0",
                mode: row
                    .get(5)
                    .map_or(ShareMode::ReadWrite, |c| ShareMode::from_code(c)),
                shared_devices,
            });
        }
        if needs_save {
            self.save_shares(&entries);
        }
        entries
    }

    /// Saves shared folder configuration to `<base_data_dir>/shares.cfg`.
    pub fn save_shares(&self, shares: &[ShareConfigEntry]) {
        let rows: Vec<Vec<String>> = shares
            .iter()
            .map(|s| {
                let devs: Vec<String> = s.shared_devices.iter().map(DeviceId::to_hex).collect();
                vec![
                    s.folder_id.clone(),
                    s.label.clone(),
                    s.target_path.clone(),
                    if s.enabled { "1" } else { "0" }.to_owned(),
                    devs.join(","),
                    s.mode.code().to_owned(),
                ]
            })
            .collect();
        self.save_rows("shares.cfg", &rows);
    }

    /// Loads declined folder offers from `<base_data_dir>/ignored_folders.cfg`.
    pub fn load_ignored_folders(&self) -> Vec<IgnoredFolder> {
        self.load_rows("ignored_folders.cfg")
            .into_iter()
            .filter_map(|row| {
                if row.len() < 2 {
                    return None;
                }
                Some(IgnoredFolder {
                    folder_id: row[0].clone(),
                    device_id: DeviceId::from_hex(row[1].trim())?,
                })
            })
            .collect()
    }

    /// Saves declined folder offers to `<base_data_dir>/ignored_folders.cfg`.
    pub fn save_ignored_folders(&self, ignored: &[IgnoredFolder]) {
        let rows: Vec<Vec<String>> = ignored
            .iter()
            .map(|f| vec![f.folder_id.clone(), f.device_id.to_hex()])
            .collect();
        self.save_rows("ignored_folders.cfg", &rows);
    }

    /// Loads declined incoming devices from `<base_data_dir>/ignored_devices.cfg`.
    pub fn load_ignored_devices(&self) -> Vec<DeviceId> {
        self.load_rows("ignored_devices.cfg")
            .into_iter()
            .filter_map(|row| {
                if row.len() == 1 {
                    DeviceId::from_hex(row[0].trim())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Saves declined incoming devices to `<base_data_dir>/ignored_devices.cfg`.
    pub fn save_ignored_devices(&self, ignored: &[DeviceId]) {
        let rows: Vec<Vec<String>> = ignored.iter().map(|id| vec![id.to_hex()]).collect();
        self.save_rows("ignored_devices.cfg", &rows);
    }

    /// Loads the chosen colour theme mode from `<base_data_dir>/theme.cfg`.
    /// Defaults to `ThemeMode::Oled` if the file does not exist or contains invalid data.
    pub fn load_theme(&self) -> ThemeMode {
        self.load_rows("theme.cfg")
            .first()
            .and_then(|row| row.first())
            .and_then(|value| ThemeMode::parse(value))
            .unwrap_or(ThemeMode::Oled)
    }

    /// Saves the chosen colour theme mode to `<base_data_dir>/theme.cfg`.
    pub fn save_theme(&self, theme: ThemeMode) {
        self.save_rows("theme.cfg", &[vec![theme.as_str().to_owned()]]);
    }

    /// Loads the UI language preference from `<base_data_dir>/language.cfg`.
    /// Defaults to `LanguagePreference::System` if the file is missing or invalid.
    pub fn load_language(&self) -> LanguagePreference {
        self.load_rows("language.cfg")
            .first()
            .and_then(|row| row.first())
            .and_then(|value| LanguagePreference::parse(value))
            .unwrap_or_default()
    }

    /// Saves the UI language preference to `<base_data_dir>/language.cfg`.
    pub fn save_language(&self, pref: LanguagePreference) {
        self.save_rows("language.cfg", &[vec![pref.as_str().to_owned()]]);
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

/// Components of a path, comparable whatever its spelling (`ux0:data`, `ux0:/data/`, letter
/// case: PS Vita storage is case-insensitive).
fn path_components(path: &str) -> Vec<String> {
    path.split(['/', ':'])
        .filter(|c| !c.is_empty() && *c != ".")
        .map(str::to_lowercase)
        .collect()
}

/// Replaces `path` with `content` through a temp file, keeping the previous version as
/// `<path>.bak` until the new one is in place (see `syncthing_core::storage::atomic`).
fn write_replacing(path: &str, content: &str) {
    let tmp = format!("{}.tmp", path);
    if write_file_synced(Path::new(&tmp), content.as_bytes()).is_ok() {
        let _ = replace_file(
            Path::new(&tmp),
            Path::new(path),
            Path::new(&backup_path(path)),
        );
    }
}

/// Reads `path`, first restoring it from its backup if a replacement was interrupted.
fn read_recovering(path: &str) -> std::io::Result<String> {
    recover_replaced_file(Path::new(path), Path::new(&backup_path(path)));
    fs::read_to_string(path)
}

fn backup_path(path: &str) -> String {
    format!("{}.bak", path)
}
