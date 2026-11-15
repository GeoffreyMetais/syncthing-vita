//! Reading and atomically writing `config.toml`.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use super::model::Config;
use crate::paths::CONFIG_FILE;

/// Loads `home/config.toml`; a missing file yields the default configuration.
pub fn load_config(home: &Path) -> io::Result<Config> {
    let path = home.join(CONFIG_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            )
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e),
    }
}

/// Writes `home/config.toml` through a temporary file, then renames it into place.
pub fn save_config(home: &Path, cfg: &Config) -> io::Result<()> {
    let text = toml::to_string_pretty(cfg).map_err(|e| io::Error::other(e.to_string()))?;
    let path = home.join(CONFIG_FILE);
    let tmp_path = home.join(format!("{CONFIG_FILE}.tmp"));
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, &path)
}
