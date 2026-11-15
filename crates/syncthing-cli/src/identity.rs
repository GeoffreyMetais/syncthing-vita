//! Persistent device identity: a 32-byte seed stored in the home directory.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

use syncthing_core::DeviceIdentity;

use crate::paths::SEED_FILE;

const SEED_LEN: usize = 32;
const URANDOM: &str = "/dev/urandom";

/// Loads the device identity from `home/device_seed.bin`, creating the seed on first use.
///
/// A missing seed is generated from `/dev/urandom`; there is no fallback random source.
pub fn load_or_create_identity(home: &Path) -> io::Result<DeviceIdentity> {
    let seed_path = home.join(SEED_FILE);
    let seed = match fs::read(&seed_path) {
        Ok(bytes) => seed_from_bytes(&bytes, &seed_path)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let seed = random_seed()?;
            write_seed(home, &seed_path, &seed)?;
            seed
        }
        Err(e) => return Err(e),
    };
    DeviceIdentity::from_seed(&seed)
}

fn seed_from_bytes(bytes: &[u8], path: &Path) -> io::Result<[u8; SEED_LEN]> {
    <[u8; SEED_LEN]>::try_from(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} must be exactly {SEED_LEN} bytes, found {}",
                path.display(),
                bytes.len()
            ),
        )
    })
}

fn random_seed() -> io::Result<[u8; SEED_LEN]> {
    let mut seed = [0u8; SEED_LEN];
    File::open(URANDOM)?.read_exact(&mut seed)?;
    Ok(seed)
}

/// Writes the seed through a temporary file, then renames it into place.
fn write_seed(home: &Path, seed_path: &Path, seed: &[u8; SEED_LEN]) -> io::Result<()> {
    let tmp_path = home.join(format!("{SEED_FILE}.tmp"));
    let _ = fs::remove_file(&tmp_path);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    // The seed is the private key: never readable by other users, not even briefly.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    {
        let mut file = options.open(&tmp_path)?;
        file.write_all(seed)?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, seed_path)
}
