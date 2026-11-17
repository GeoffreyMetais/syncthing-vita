//! Background sync plugin (taiHEN): where its modules live, its status as shown in `Settings`,
//! the operations the user can ask for, and their outcomes.
//!
//! Two modules are bundled in the VPK (`app0:plugins/`) and installed side by side in the
//! active `tai` directory: the resident launcher, listed under `*main` in `config.txt`, and the
//! sync worker it loads on demand (never listed in `config.txt`).

/// Resident launcher, loaded by taiHEN into `SceShell`.
pub const LAUNCHER_MODULE: &str = "syncthing_launcher.suprx";
/// Sync worker, loaded by the launcher for one sync cycle, from its own directory.
pub const WORKER_MODULE: &str = "syncthing_worker.suprx";
/// Both modules, in installation order.
pub const PLUGIN_MODULES: [&str; 2] = [LAUNCHER_MODULE, WORKER_MODULE];
/// Directory of the VPK holding the bundled modules.
pub const PLUGIN_BUNDLE_DIR: &str = "app0:plugins/";
/// taiHEN directory on the memory card (its `config.txt` wins when it exists).
pub const TAI_DIR_UX0: &str = "ux0:tai/";
/// taiHEN directory on the internal `ur0:` partition (used when `ux0:tai/config.txt` is absent).
pub const TAI_DIR_UR0: &str = "ur0:tai/";
/// Both `tai` directories.
pub const TAI_DIRS: [&str; 2] = [TAI_DIR_UX0, TAI_DIR_UR0];
/// taiHEN configuration file, in a `tai` directory.
pub const TAI_CONFIG_FILE: &str = "config.txt";
/// Copy of `config.txt` taken before each modification, in the same directory.
pub const TAI_CONFIG_BACKUP_FILE: &str = "config.txt.bak";

/// `tai` directory whose `config.txt` taiHEN reads: `ux0:tai/` when its `config.txt` exists,
/// `ur0:tai/` otherwise.
pub fn active_tai_dir(ux0_config_exists: bool) -> &'static str {
    if ux0_config_exists {
        TAI_DIR_UX0
    } else {
        TAI_DIR_UR0
    }
}

/// Paths a `config.txt` entry may name for our modules, other than the launcher in
/// `active_dir` (the worker anywhere, the launcher in the other `tai` directory): entries to
/// drop when installing into `active_dir`.
pub fn stale_entries(active_dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    for dir in TAI_DIRS {
        for module in PLUGIN_MODULES {
            if !(dir == active_dir && module == LAUNCHER_MODULE) {
                out.push(format!("{dir}{module}"));
            }
        }
    }
    out
}

/// Every path a `config.txt` entry may name for our modules (removed on uninstall).
pub fn all_module_entries() -> Vec<String> {
    TAI_DIRS
        .iter()
        .flat_map(|dir| PLUGIN_MODULES.iter().map(move |m| format!("{dir}{m}")))
        .collect()
}

/// Status of the background sync plugin, as shown by its `Settings` card.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PluginStatus {
    /// No launcher in the active `tai` directory, or no `*main` entry for it.
    #[default]
    NotInstalled,
    /// Installed, but the launcher is not running yet: taiHEN loads it at the next boot.
    InstalledNeedsReboot,
    /// Running, background sync allowed (`enabled=1`).
    Enabled,
    /// Running, background sync switched off (`enabled=0`).
    Disabled,
}

impl PluginStatus {
    /// Status from what is on the memory card (`installed`), whether the launcher answers
    /// (`launcher_alive`) and the `enabled` flag of `plugin.cfg`.
    pub fn resolve(installed: bool, launcher_alive: bool, enabled: bool) -> Self {
        match (installed, launcher_alive, enabled) {
            (false, _, _) => Self::NotInstalled,
            (true, false, _) => Self::InstalledNeedsReboot,
            (true, true, true) => Self::Enabled,
            (true, true, false) => Self::Disabled,
        }
    }

    pub fn is_installed(self) -> bool {
        self != Self::NotInstalled
    }
}

/// What the `Settings` card shows about the plugin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PluginInfo {
    pub status: PluginStatus,
    /// `enabled` flag of `plugin.cfg` (what the launcher does once running).
    pub enabled: bool,
    /// The launcher is loaded in `SceShell` (it stays loaded until the next reboot, even
    /// once uninstalled).
    pub launcher_loaded: bool,
}

impl PluginInfo {
    pub fn new(installed: bool, launcher_loaded: bool, enabled: bool) -> Self {
        Self {
            status: PluginStatus::resolve(installed, launcher_loaded, enabled),
            enabled,
            launcher_loaded,
        }
    }

    /// Uninstalled while its launcher is still loaded: a reboot unloads it.
    pub fn unload_pending(&self) -> bool {
        self.status == PluginStatus::NotInstalled && self.launcher_loaded
    }

    /// Buttons of the `Settings` card: `[Install]`, or `[Enable / Disable, Uninstall]`.
    pub fn action_count(&self) -> usize {
        if self.status.is_installed() {
            2
        } else {
            1
        }
    }
}

/// Operation on the plugin asked from `Settings`, carried out by `PluginService`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginRequest {
    Install,
    Uninstall,
    /// Writes `enabled` to `plugin.cfg` (no reboot needed).
    SetEnabled(bool),
}

/// Outcome of a plugin operation, shown as a toast (`UiText::Plugin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginNotice {
    /// Installed and the launcher already runs (update of a running install).
    Installed,
    /// Installed: taiHEN loads it at the next boot.
    InstalledRebootNeeded,
    Uninstalled,
    /// Uninstalled: the launcher stays loaded until the next reboot (it stays idle).
    UninstalledRebootNeeded,
    /// Background sync allowed again: it starts once this app is closed.
    Enabled,
    Disabled,
    /// This build carries no plugin modules (`app0:plugins/`).
    BundleMissing,
    /// Neither `ux0:tai/config.txt` nor `ur0:tai/config.txt` exists (no taiHEN).
    TaiConfigMissing,
    /// A file could not be written or removed (details in the log).
    WriteFailed,
}
