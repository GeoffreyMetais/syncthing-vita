//! Syncthing Device Identifier (32 bytes = SHA-256 of the device's X.509 certificate),
//! its canonical 56-character Luhn-mod-32 string form, hex persistence and short-ID matching.
//!
//! - [`base32`]: RFC 4648 Base32 and Luhn mod 32 primitives
//! - [`parse`] — parsing of user-entered IDs (full, raw Base32, hex, shortened prefixes)

mod base32;
mod parse;

pub use parse::DeviceIdParseError;

use std::fmt;

use base32::{base32_encode, luhn_mod32_check_char};

/// Magic sentinel stored in bytes `24..32` when a `DeviceId` represents a user-entered
/// shortened Device ID prefix (4..51 characters) awaiting full peer certificate resolution.
const SHORT_ID_SENTINEL: &[u8; 8] = b"VITASHRT";

/// Syncthing Device Identifier (32 bytes = SHA-256 of X.509 certificate).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub [u8; 32]);

impl DeviceId {
    /// Encodes the 32 bytes in RFC 4648 Base32 (52 significant characters) with
    /// the 4 Luhn mod 32 check characters used by Syncthing (56 characters, 8 groups of 7).
    pub fn to_syncthing_string(&self) -> String {
        let b32 = base32_encode(&self.0); // 52 significant characters
        let mut with_check = String::with_capacity(63);

        // Syncthing chunks the 52 Base32 characters into 4 blocks of 13 characters,
        // appends 1 Luhn mod 32 check character to each block (4 * 14 = 56 chars),
        // and inserts a hyphen every 7 characters.
        let mut raw56 = String::with_capacity(56);
        for chunk in b32.as_bytes().chunks(13) {
            for &c in chunk {
                raw56.push(c as char);
            }
            if chunk.len() == 13 {
                raw56.push(luhn_mod32_check_char(chunk) as char);
            }
        }

        for (i, ch) in raw56.chars().enumerate() {
            if i > 0 && i % 7 == 0 {
                with_check.push('-');
            }
            with_check.push(ch);
        }
        with_check
    }

    /// Returns a short version (first 7 characters) for compact logs.
    pub fn short(&self) -> String {
        let full = self.to_syncthing_string();
        full.chars().take(7).collect()
    }

    /// Returns `true` if this `DeviceId` was created from a shortened ID prefix (`4..51` chars)
    /// and is awaiting upgrade to a full 32-byte peer certificate hash.
    pub fn is_short_placeholder(&self) -> bool {
        &self.0[24..32] == SHORT_ID_SENTINEL && (4..=37).contains(&self.0[23])
    }

    /// Returns the uppercase Base32 Syncthing prefix (without hyphens) for a shortened placeholder,
    /// or all 56 characters for a full `DeviceId`.
    pub fn syncthing_prefix(&self) -> String {
        let clean: String = self
            .to_syncthing_string()
            .chars()
            .filter(|&c| c != '-')
            .collect();
        if self.is_short_placeholder() {
            let n = (self.0[23] as usize).min(clean.len());
            clean[..n].to_owned()
        } else {
            clean
        }
    }

    /// Returns `true` if `self` and `other` represent the same Syncthing device, supporting
    /// prefix matching when either `self` or `other` is a shortened Device ID placeholder.
    pub fn matches_peer(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }
        if self.is_short_placeholder() && !other.is_short_placeholder() {
            let prefix = self.syncthing_prefix();
            let full_clean: String = other
                .to_syncthing_string()
                .chars()
                .filter(|&c| c != '-')
                .collect();
            return full_clean.starts_with(&prefix);
        }
        if other.is_short_placeholder() && !self.is_short_placeholder() {
            return other.matches_peer(self);
        }
        if self.is_short_placeholder() && other.is_short_placeholder() {
            let p1 = self.syncthing_prefix();
            let p2 = other.syncthing_prefix();
            return p1.starts_with(&p2) || p2.starts_with(&p1);
        }
        false
    }

    /// Encodes the 32-byte DeviceId as a 64-character lowercase hex string for config persistence.
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    /// Parses a 64-character hex string back into a 32-byte `DeviceId`.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let clean = hex.trim();
        if clean.len() != 64 {
            return None;
        }
        let mut raw = [0u8; 32];
        for (i, item) in raw.iter_mut().enumerate() {
            *item = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(DeviceId(raw))
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({})", self.to_syncthing_string())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_syncthing_string())
    }
}
