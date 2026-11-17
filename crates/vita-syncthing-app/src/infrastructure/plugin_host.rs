//! Files of the background sync plugin (`PluginHost`): the modules bundled in `app0:plugins/`,
//! the `tai` directories of `ux0:` / `ur0:`, and `plugin.cfg` in the app's data directory, plus
//! the probe telling whether the launcher is loaded (`LauncherProbe`).
//!
//! Paths are PS Vita device paths (`ux0:tai/config.txt`). On the PS Vita they are used as
//! they are; a host `PluginHost` maps them below a root directory
//! (`<root>/ux0/tai/config.txt`), so tests and host builds drive the same code.

use std::fs;
use std::io;
use std::path::Path;

use syncthing_core::storage::atomic::{recover_replaced_file, replace_file, write_file_synced};

use crate::domain::PluginConfig;

/// Name of `plugin.cfg` in the app's data directory.
pub const PLUGIN_CFG_FILE: &str = "plugin.cfg";

/// Suffix of the previous version of a `tai` file kept while it is replaced (never
/// `config.txt.bak`: that name holds the user-visible backup).
const TAI_REPLACE_BACKUP_SUFFIX: &str = ".old";
/// Suffix of the previous version of `plugin.cfg` kept while it is replaced (as the other
/// `.cfg` files of the app).
const CFG_REPLACE_BACKUP_SUFFIX: &str = ".bak";

/// Tells whether the plugin's launcher is loaded in `SceShell`.
pub trait LauncherProbe {
    fn launcher_alive(&self) -> bool;
}

/// Probe with a fixed answer (host builds and tests).
#[derive(Debug, Clone, Copy)]
pub struct FixedLauncherProbe(pub bool);

impl LauncherProbe for FixedLauncherProbe {
    fn launcher_alive(&self) -> bool {
        self.0
    }
}

/// File access and launcher probe for `application::PluginService`.
pub struct PluginHost {
    /// Directory the device paths are mapped below (`None` on the PS Vita: used as is).
    device_root: Option<String>,
    /// Directory holding `plugin.cfg` (`ux0:data/syncthing` on the PS Vita).
    data_dir: String,
    probe: Box<dyn LauncherProbe>,
}

impl PluginHost {
    /// Host used by the app: the real devices and kernel probe on the PS Vita; on a host
    /// build, devices below `<data_dir>/devices` and a launcher that never runs.
    pub fn system(data_dir: &str) -> Self {
        #[cfg(target_os = "vita")]
        {
            Self {
                device_root: None,
                data_dir: data_dir.to_owned(),
                probe: Box::new(super::plugin_mutex::KernelLauncherProbe),
            }
        }
        #[cfg(not(target_os = "vita"))]
        {
            Self::rooted(
                &format!("{data_dir}/devices"),
                data_dir,
                Box::new(FixedLauncherProbe(false)),
            )
        }
    }

    /// Host whose device paths live below `root` (`<root>/ux0/...`, `<root>/app0/...`).
    pub fn rooted(root: &str, data_dir: &str, probe: Box<dyn LauncherProbe>) -> Self {
        Self {
            device_root: Some(root.trim_end_matches('/').to_owned()),
            data_dir: data_dir.to_owned(),
            probe,
        }
    }

    /// Filesystem path of the device path `path` (`ux0:tai/x` -> `<root>/ux0/tai/x`).
    pub fn local_path(&self, path: &str) -> String {
        let Some(root) = &self.device_root else {
            return path.to_owned();
        };
        match path.split_once(':') {
            Some((device, rest)) => format!("{root}/{device}/{}", rest.trim_start_matches('/')),
            None => format!("{root}/{}", path.trim_start_matches('/')),
        }
    }

    /// True if the device file `path` exists (an interrupted replacement is finished first).
    pub fn exists(&self, path: &str) -> bool {
        let local = self.local_path(path);
        recover(&local, TAI_REPLACE_BACKUP_SUFFIX);
        fs::metadata(&local).is_ok()
    }

    /// Contents of the device file `path` (an interrupted replacement is finished first).
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let local = self.local_path(path);
        recover(&local, TAI_REPLACE_BACKUP_SUFFIX);
        fs::read(&local)
    }

    /// Text of the device file `path` (`None` if it cannot be read as UTF-8 text).
    pub fn read_text(&self, path: &str) -> Option<String> {
        self.read(path)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }

    /// Replaces the device file `path` with `bytes` (crash-safe: temp file, previous version
    /// kept until the new one is in place).
    pub fn write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let local = self.local_path(path);
        if let Some(parent) = Path::new(&local).parent() {
            fs::create_dir_all(parent)?;
        }
        write_replacing(&local, bytes, TAI_REPLACE_BACKUP_SUFFIX)
    }

    /// Removes the device file `path` (and a leftover of an interrupted replacement).
    pub fn remove(&self, path: &str) -> io::Result<()> {
        let local = self.local_path(path);
        let _ = fs::remove_file(backup(&local, TAI_REPLACE_BACKUP_SUFFIX));
        match fs::remove_file(&local) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    fn plugin_cfg_path(&self) -> String {
        format!("{}/{PLUGIN_CFG_FILE}", self.data_dir)
    }

    /// `plugin.cfg` (defaults when it is missing).
    pub fn load_plugin_config(&self) -> PluginConfig {
        let path = self.plugin_cfg_path();
        recover(&path, CFG_REPLACE_BACKUP_SUFFIX);
        fs::read_to_string(&path)
            .map(|text| PluginConfig::parse(&text))
            .unwrap_or_default()
    }

    /// Writes `plugin.cfg` (crash-safe).
    pub fn save_plugin_config(&self, cfg: &PluginConfig) -> io::Result<()> {
        fs::create_dir_all(&self.data_dir)?;
        write_replacing(
            &self.plugin_cfg_path(),
            cfg.serialize().as_bytes(),
            CFG_REPLACE_BACKUP_SUFFIX,
        )
    }

    pub fn launcher_alive(&self) -> bool {
        self.probe.launcher_alive()
    }
}

fn backup(path: &str, suffix: &str) -> String {
    format!("{path}{suffix}")
}

fn recover(path: &str, suffix: &str) {
    recover_replaced_file(Path::new(path), Path::new(&backup(path, suffix)));
}

fn write_replacing(path: &str, bytes: &[u8], suffix: &str) -> io::Result<()> {
    let tmp = format!("{path}.tmp");
    write_file_synced(Path::new(&tmp), bytes)?;
    replace_file(
        Path::new(&tmp),
        Path::new(path),
        Path::new(&backup(path, suffix)),
    )
}
