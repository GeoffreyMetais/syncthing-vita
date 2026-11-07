//! PS Vita cryptographic identity persistence (`device_seed.bin` & `cert.der`)
//! using hardware RNG (`sceKernelGetRandomNumber`) on first install.

use std::fs;
use std::io;

use syncthing_core::VitaIdentity;

/// Manages loading or generating the unique ECDSA P-256 device identity on `ux0:`.
pub struct IdentityStore;

impl IdentityStore {
    /// Generates 32 bytes of cryptographic entropy using PS Vita's `sceKernelGetRandomNumber`
    /// (or `/dev/urandom` on host platforms).
    pub fn generate_random_seed() -> [u8; 32] {
        let mut seed = [0u8; 32];

        #[cfg(target_os = "vita")]
        unsafe {
            let ret = crate::infrastructure::vita_system::vita_ffi::sceKernelGetRandomNumber(
                seed.as_mut_ptr() as *mut libc::c_void,
                seed.len() as u32,
            );
            if ret == 0 && seed.iter().any(|&b| b != 0) {
                return seed;
            }
        }

        #[cfg(not(target_os = "vita"))]
        if let Ok(mut urandom) = fs::File::open("/dev/urandom") {
            use std::io::Read;
            if urandom.read_exact(&mut seed).is_ok() && seed.iter().any(|&b| b != 0) {
                return seed;
            }
        }

        // Fallback entropy mixing using high-resolution timestamps and stack addresses
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0x5649_5441_2026_1106);
        let mut state = (nanos as u64)
            ^ ((&seed as *const _ as usize) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        for byte in &mut seed {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            *byte = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 56) as u8;
        }
        seed
    }

    /// Loads the existing 32-byte seed from `<base_data_dir>/device_seed.bin`, or creates
    /// a fresh hardware-random seed on first launch, and exports `cert.der`.
    pub fn load_or_create(base_data_dir: &str) -> io::Result<VitaIdentity> {
        fs::create_dir_all(base_data_dir)?;
        let seed_path = format!("{}/device_seed.bin", base_data_dir);
        let cert_path = format!("{}/cert.der", base_data_dir);

        let seed: [u8; 32] = if let Ok(bytes) = fs::read(&seed_path) {
            if bytes.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                arr
            } else {
                let fresh_seed = Self::generate_random_seed();
                fs::write(&seed_path, &fresh_seed)?;
                fresh_seed
            }
        } else {
            let fresh_seed = Self::generate_random_seed();
            fs::write(&seed_path, &fresh_seed)?;
            fresh_seed
        };

        let identity = VitaIdentity::from_seed(&seed)?;
        let _ = fs::write(&cert_path, &identity.cert_der);
        Ok(identity)
    }
}
