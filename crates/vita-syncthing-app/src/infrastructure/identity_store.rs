//! PS Vita cryptographic identity persistence (`device_seed.bin` & `cert.der`)
//! using hardware RNG (`sceKernelGetRandomNumber`) on first install.

use std::fs;
use std::io;
use std::path::Path;

use syncthing_core::storage::atomic::write_file_synced;
use syncthing_core::VitaIdentity;

/// Manages loading or generating the unique ECDSA P-256 device identity on `ux0:`.
pub struct IdentityStore;

impl IdentityStore {
    /// Generates 32 bytes of cryptographic entropy using PS Vita's `sceKernelGetRandomNumber`
    /// (or `/dev/urandom` on host platforms). There is no fallback: a seed derived from
    /// anything guessable (clock, addresses) would give a private key an attacker can
    /// recompute, so a failing random source is an error.
    pub fn generate_random_seed() -> io::Result<[u8; 32]> {
        let mut seed = [0u8; 32];

        #[cfg(target_os = "vita")]
        let filled = unsafe {
            crate::infrastructure::vita_system::vita_ffi::sceKernelGetRandomNumber(
                seed.as_mut_ptr() as *mut libc::c_void,
                seed.len() as u32,
            ) == 0
        };

        #[cfg(not(target_os = "vita"))]
        let filled = {
            use std::io::Read;
            fs::File::open("/dev/urandom")
                .and_then(|mut urandom| urandom.read_exact(&mut seed))
                .is_ok()
        };

        if filled && seed.iter().any(|&b| b != 0) {
            Ok(seed)
        } else {
            Err(io::Error::other(
                "system random number generator unavailable",
            ))
        }
    }

    /// Loads the existing 32-byte seed from `<base_data_dir>/device_seed.bin`, or creates
    /// a fresh hardware-random seed on first launch, and exports `cert.der`.
    pub fn load_or_create(base_data_dir: &str) -> io::Result<VitaIdentity> {
        fs::create_dir_all(base_data_dir)?;
        let seed_path = format!("{}/device_seed.bin", base_data_dir);
        let cert_path = format!("{}/cert.der", base_data_dir);

        let stored: Option<[u8; 32]> = fs::read(&seed_path)
            .ok()
            .and_then(|bytes| bytes.try_into().ok());
        let seed = match stored {
            Some(seed) => seed,
            None => {
                let fresh_seed = Self::generate_random_seed()?;
                write_file_synced(Path::new(&seed_path), &fresh_seed)?;
                fresh_seed
            }
        };

        let identity = VitaIdentity::from_seed(&seed)?;
        let _ = fs::write(&cert_path, &identity.cert_der);
        Ok(identity)
    }
}
