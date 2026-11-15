//! Location of the configuration directory (identity, `config.toml`, folder databases).

use std::io;
use std::path::{Path, PathBuf};

/// Name of the configuration file inside the home directory.
pub const CONFIG_FILE: &str = "config.toml";
/// Name of the 32-byte device seed inside the home directory.
pub const SEED_FILE: &str = "device_seed.bin";

const HOME_ENV: &str = "SYNCTHING_CLI_HOME";
const APP_DIR: &str = "syncthing-cli";

/// Resolves the home directory: `explicit` (`--home`), then `$SYNCTHING_CLI_HOME`,
/// then [`default_home`]. The directory is created if missing.
pub fn resolve_home(explicit: Option<PathBuf>) -> io::Result<PathBuf> {
    let home = match explicit {
        Some(path) => path,
        None => match std::env::var_os(HOME_ENV) {
            Some(value) if !value.is_empty() => PathBuf::from(value),
            _ => default_home()?,
        },
    };
    std::fs::create_dir_all(&home)?;
    Ok(home)
}

/// Platform configuration directory: `~/Library/Application Support/syncthing-cli` on macOS,
/// `$XDG_CONFIG_HOME/syncthing-cli` (or `~/.config/syncthing-cli`) elsewhere.
pub fn default_home() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;

    if cfg!(target_os = "macos") {
        return Ok(home.join("Library/Application Support").join(APP_DIR));
    }
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value).join(APP_DIR)),
        _ => Ok(home.join(".config").join(APP_DIR)),
    }
}

/// Directory holding the folder databases.
pub fn db_dir(home: &Path) -> PathBuf {
    home.join("db")
}
