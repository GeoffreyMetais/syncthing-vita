//! Background sync plugin use cases (`PluginService`): install the bundled modules into the
//! active `tai` directory and list the launcher under `*main` in `config.txt`, uninstall them,
//! switch background sync on / off in `plugin.cfg`, and refresh the status shown in `Settings`.
//!
//! Switching background sync on never starts a sync while this app runs: the app holds the
//! `SyncthingGuiRunning` mutex (`infrastructure::GuiRunningLock`) and the plugin waits for it to
//! disappear. Uninstalling leaves `enabled=0` in `plugin.cfg`, so a launcher still loaded until
//! the next reboot stays idle even if it finds its files.

use crate::application::ShareService;
use crate::domain::plugin::{
    active_tai_dir, all_module_entries, stale_entries, LAUNCHER_MODULE, PLUGIN_BUNDLE_DIR,
    PLUGIN_MODULES, TAI_CONFIG_BACKUP_FILE, TAI_CONFIG_FILE, TAI_DIRS, TAI_DIR_UX0,
};
use crate::domain::{
    tai_config, AppState, PluginInfo, PluginNotice, PluginRequest, PluginStatus, UiText,
};
use crate::infrastructure::{ConfigRepository, PluginHost};

/// Why a plugin operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// A module is missing from `app0:plugins/` (build without the plugin).
    BundleMissing(String),
    /// No taiHEN `config.txt` in `ux0:tai/` nor `ur0:tai/`.
    TaiConfigMissing,
    /// A file could not be written or removed: `(path, error)`.
    Io(String, String),
}

impl PluginError {
    /// Toast shown for this failure.
    pub fn notice(&self) -> PluginNotice {
        match self {
            Self::BundleMissing(_) => PluginNotice::BundleMissing,
            Self::TaiConfigMissing => PluginNotice::TaiConfigMissing,
            Self::Io(..) => PluginNotice::WriteFailed,
        }
    }

    /// Protocol log line (English).
    pub fn log_line(&self) -> String {
        match self {
            Self::BundleMissing(path) => format!("Plugin: bundled module missing: {path}"),
            Self::TaiConfigMissing => {
                "Plugin: no taiHEN config.txt in ux0:tai/ nor ur0:tai/".to_owned()
            }
            Self::Io(path, err) => format!("Plugin: cannot write {path}: {err}"),
        }
    }
}

fn io_error(path: &str) -> impl FnOnce(std::io::Error) -> PluginError + '_ {
    move |e| PluginError::Io(path.to_owned(), e.to_string())
}

pub struct PluginService;

impl PluginService {
    /// `tai` directory whose `config.txt` taiHEN reads.
    pub fn active_tai_dir(host: &PluginHost) -> &'static str {
        active_tai_dir(host.exists(&format!("{TAI_DIR_UX0}{TAI_CONFIG_FILE}")))
    }

    /// Status of the plugin from the files on the memory card, the launcher probe and
    /// `plugin.cfg`.
    pub fn status(host: &PluginHost) -> PluginInfo {
        let dir = Self::active_tai_dir(host);
        let launcher = format!("{dir}{LAUNCHER_MODULE}");
        let listed = host
            .read_text(&format!("{dir}{TAI_CONFIG_FILE}"))
            .is_some_and(|config| tai_config::has_main_entry(&config, &launcher));
        let installed = listed && host.exists(&launcher);
        PluginInfo::new(
            installed,
            host.launcher_alive(),
            host.load_plugin_config().enabled,
        )
    }

    /// Updates `state.plugin` from `host`.
    pub fn refresh(state: &mut AppState, host: &PluginHost) {
        state.plugin = Self::status(host);
    }

    /// Carries out `request` (confirmed in `Settings`), refreshes the status, logs the outcome
    /// and shows it as a toast.
    pub fn execute(
        state: &mut AppState,
        repo: &ConfigRepository,
        host: &PluginHost,
        request: PluginRequest,
    ) {
        let was_loaded = host.launcher_alive();
        let result = match request {
            PluginRequest::Install => Self::install(host).map(|dir| {
                ShareService::log_event(state, repo, &format!("Plugin installed in {dir}"));
            }),
            PluginRequest::Uninstall => Self::uninstall(host).map(|()| {
                ShareService::log_event(state, repo, "Plugin uninstalled.");
            }),
            PluginRequest::SetEnabled(enabled) if state.plugin.status.is_installed() => {
                Self::set_enabled(host, enabled).map(|()| {
                    let line = format!("Plugin background sync enabled={}", u8::from(enabled));
                    ShareService::log_event(state, repo, &line);
                })
            }
            // Stale frame: the plugin is no longer installed.
            PluginRequest::SetEnabled(_) => return,
        };
        Self::refresh(state, host);
        let notice = match result {
            Ok(()) => Self::success_notice(request, state.plugin.status, was_loaded),
            Err(err) => {
                ShareService::log_event(state, repo, &err.log_line());
                err.notice()
            }
        };
        state.show_toast(UiText::Plugin(notice));
    }

    fn success_notice(request: PluginRequest, status: PluginStatus, loaded: bool) -> PluginNotice {
        match request {
            PluginRequest::Install if status == PluginStatus::InstalledNeedsReboot => {
                PluginNotice::InstalledRebootNeeded
            }
            PluginRequest::Install => PluginNotice::Installed,
            PluginRequest::Uninstall if loaded => PluginNotice::UninstalledRebootNeeded,
            PluginRequest::Uninstall => PluginNotice::Uninstalled,
            PluginRequest::SetEnabled(true) => PluginNotice::Enabled,
            PluginRequest::SetEnabled(false) => PluginNotice::Disabled,
        }
    }

    /// Copies the bundled modules into the active `tai` directory, lists the launcher under
    /// `*main` (backing `config.txt` up first) and enables background sync. Returns the
    /// directory used. Nothing is written when a module or `config.txt` is missing.
    pub fn install(host: &PluginHost) -> Result<&'static str, PluginError> {
        let mut modules = Vec::with_capacity(PLUGIN_MODULES.len());
        for module in PLUGIN_MODULES {
            let path = format!("{PLUGIN_BUNDLE_DIR}{module}");
            match host.read(&path) {
                Ok(bytes) if !bytes.is_empty() => modules.push((module, bytes)),
                _ => return Err(PluginError::BundleMissing(path)),
            }
        }
        let dir = Self::active_tai_dir(host);
        let config_path = format!("{dir}{TAI_CONFIG_FILE}");
        let config = host
            .read_text(&config_path)
            .ok_or(PluginError::TaiConfigMissing)?;

        for (module, bytes) in &modules {
            let path = format!("{dir}{module}");
            host.write(&path, bytes).map_err(io_error(&path))?;
        }
        let stale = stale_entries(dir);
        let stale: Vec<&str> = stale.iter().map(String::as_str).collect();
        let updated =
            tai_config::add_main_entry(&config, &format!("{dir}{LAUNCHER_MODULE}"), &stale);
        Self::replace_config(host, dir, &config, &updated)?;
        Self::set_enabled(host, true)?;
        Ok(dir)
    }

    /// Removes our entries from both `config.txt` files (backed up first) and our modules
    /// from both `tai` directories, and leaves `enabled=0` in `plugin.cfg`.
    pub fn uninstall(host: &PluginHost) -> Result<(), PluginError> {
        let entries = all_module_entries();
        let entries: Vec<&str> = entries.iter().map(String::as_str).collect();
        for dir in TAI_DIRS {
            if let Some(config) = host.read_text(&format!("{dir}{TAI_CONFIG_FILE}")) {
                let updated = tai_config::remove_entries(&config, &entries);
                Self::replace_config(host, dir, &config, &updated)?;
            }
            for module in PLUGIN_MODULES {
                let path = format!("{dir}{module}");
                if host.exists(&path) {
                    host.remove(&path).map_err(io_error(&path))?;
                }
            }
        }
        Self::set_enabled(host, false)
    }

    /// Writes `enabled` to `plugin.cfg`, keeping its other settings.
    pub fn set_enabled(host: &PluginHost, enabled: bool) -> Result<(), PluginError> {
        let mut cfg = host.load_plugin_config();
        cfg.enabled = enabled;
        host.save_plugin_config(&cfg)
            .map_err(|e| PluginError::Io("plugin.cfg".to_owned(), e.to_string()))
    }

    /// Replaces `<dir>config.txt` with `updated` when it differs from `current`, after
    /// copying `current` to `<dir>config.txt.bak`.
    fn replace_config(
        host: &PluginHost,
        dir: &str,
        current: &str,
        updated: &str,
    ) -> Result<(), PluginError> {
        if updated == current {
            return Ok(());
        }
        let backup = format!("{dir}{TAI_CONFIG_BACKUP_FILE}");
        host.write(&backup, current.as_bytes())
            .map_err(io_error(&backup))?;
        let path = format!("{dir}{TAI_CONFIG_FILE}");
        host.write(&path, updated.as_bytes())
            .map_err(io_error(&path))
    }
}
