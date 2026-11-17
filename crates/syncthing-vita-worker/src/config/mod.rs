//! The app's configuration, read by the worker: identity (`device_seed.bin`), `devices.cfg`,
//! `shares.cfg` and `ignored_folders.cfg` in the app's data directory.
//!
//! The worker never creates an identity nor writes a `.cfg` file: without the app's seed there
//! is nothing to synchronize, and the configuration belongs to the app. The only write is the
//! app's own crash recovery (`recover_replaced_file`: a replacement interrupted by a power
//! loss is completed from its backup), which the app would do on its next start anyway.

pub mod fields;
pub mod records;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use syncthing_core::storage::atomic::recover_replaced_file;
use syncthing_core::DeviceIdentity;

pub use records::{DeviceRecord, IgnoredRecord, ShareRecord};

/// Seed of the device identity, created by the app on its first start.
pub const SEED_FILE: &str = "device_seed.bin";
pub const DEVICES_FILE: &str = "devices.cfg";
pub const SHARES_FILE: &str = "shares.cfg";
pub const IGNORED_FILE: &str = "ignored_folders.cfg";
/// Directory of the folder databases and of the shares without a custom path.
pub const SYNC_DIR: &str = "sync";

/// Everything the cycle needs from the app's data directory.
#[derive(Clone)]
pub struct WorkerConfig {
    pub data_dir: PathBuf,
    pub identity: DeviceIdentity,
    pub devices: Vec<DeviceRecord>,
    pub shares: Vec<ShareRecord>,
    pub ignored: Vec<IgnoredRecord>,
}

impl WorkerConfig {
    /// Loads the configuration of `data_dir`. Fails if the identity seed is missing or
    /// invalid (the app has never run); missing `.cfg` files are empty lists, as in the app.
    pub fn load(data_dir: &Path) -> io::Result<Self> {
        let seed = fs::read(data_dir.join(SEED_FILE))?;
        let seed: [u8; 32] = seed.try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "device_seed.bin is not 32 bytes",
            )
        })?;
        let identity = DeviceIdentity::from_seed(&seed)?;
        let text = |name: &str| read_recovering(&data_dir.join(name)).unwrap_or_default();
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            identity,
            devices: records::parse_devices(&text(DEVICES_FILE)),
            shares: records::parse_shares(&text(SHARES_FILE)),
            ignored: records::parse_ignored(&text(IGNORED_FILE)),
        })
    }

    /// Root of the folder databases (`<data_dir>/sync`), the app's `FsSyncStorage` root.
    pub fn sync_root(&self) -> PathBuf {
        self.data_dir.join(SYNC_DIR)
    }
}

/// Reads `path`, first completing an interrupted replacement from `<path>.bak`, as the app's
/// `ConfigRepository` does.
fn read_recovering(path: &Path) -> io::Result<String> {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    recover_replaced_file(path, Path::new(&backup));
    fs::read_to_string(path)
}

/// Returns `true` if `path` must never be synchronized: `data_dir` itself or a folder
/// containing it (`ux0:`, `ux0:data`), which hold the identity seed and the configuration.
/// Same rule as the app's `ConfigRepository::is_protected_share_path`.
pub fn is_protected_share_path(data_dir: &Path, path: &str) -> bool {
    let base = path_components(&data_dir.to_string_lossy());
    let target = path_components(path);
    target.len() <= base.len() && base.iter().zip(&target).all(|(a, b)| a == b)
}

/// Components of a path, comparable whatever its spelling (`ux0:data`, `ux0:/data/`, letter
/// case: PS Vita storage is case-insensitive).
fn path_components(path: &str) -> Vec<String> {
    path.split(['/', ':'])
        .filter(|c| !c.is_empty() && *c != ".")
        .map(str::to_lowercase)
        .collect()
}
