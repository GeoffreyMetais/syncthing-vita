//! Parsing of user-entered Syncthing Device IDs (on-screen keyboard / manual pairing).

use std::fmt;

use super::base32::{base32_decode_prefix, luhn_mod32_check_char, BASE32_ALPHABET};
use super::{DeviceId, SHORT_ID_SENTINEL};

/// Why a user-entered Device ID was rejected. `Display` gives an English message; apps can
/// match on the variants to show a translated one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceIdParseError {
    /// Nothing was entered.
    Empty,
    /// A character outside Base32 (`A-Z`, `2-7`, plus the `0`/`1`/`8` look-alikes).
    InvalidChar(char),
    /// Fewer than 4 Base32 characters.
    TooShort,
    /// More than 56 characters (the length entered).
    TooLong(usize),
    /// Luhn check character mismatch in the group pair starting at `first_group` (1-based).
    ChecksumMismatch { first_group: usize },
    /// The Base32 payload could not be decoded.
    DecodeFailed,
}

impl fmt::Display for DeviceIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "Device ID cannot be empty"),
            Self::InvalidChar(ch) => write!(f, "Invalid character '{ch}' (use Base32 A-Z, 2-7)"),
            Self::TooShort => write!(
                f,
                "Enter at least 7 characters (Short ID) or full 56-char ID"
            ),
            Self::TooLong(len) => write!(f, "Device ID is too long ({len} chars, max 56)"),
            Self::ChecksumMismatch { first_group } => write!(
                f,
                "Typo detected in group {}-{} (check digit mismatch)",
                first_group,
                first_group + 1
            ),
            Self::DecodeFailed => write!(f, "Base32 decode failed"),
        }
    }
}

impl std::error::Error for DeviceIdParseError {}

impl DeviceId {
    /// Parses a user-entered Syncthing Device ID:
    /// - Full 56-character Syncthing ID (with or without hyphens, validated via Luhn mod 32)
    /// - Full 52-character raw Base32 ID (without Luhn check characters)
    /// - Full 64-character hexadecimal ID
    /// - Shortened Syncthing ID prefix (`4..51` Base32 characters, e.g. 7-character Short ID `MFZWI3D`)
    pub fn parse_user_id(input: &str) -> Result<Self, DeviceIdParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(DeviceIdParseError::Empty);
        }

        if let Some(hex_id) = Self::from_hex(trimmed) {
            return Ok(hex_id);
        }

        let mut clean = Vec::with_capacity(56);
        for ch in trimmed.chars() {
            if ch == '-' || ch == ' ' || ch == '_' {
                continue;
            }
            let up = match ch.to_ascii_uppercase() {
                '0' => 'O',
                '1' => 'I',
                '8' => 'B',
                c => c,
            };
            if !BASE32_ALPHABET.contains(&(up as u8)) {
                return Err(DeviceIdParseError::InvalidChar(ch));
            }
            clean.push(up as u8);
        }

        let len = clean.len();
        if len < 4 {
            return Err(DeviceIdParseError::TooShort);
        }
        if len > 56 {
            return Err(DeviceIdParseError::TooLong(len));
        }

        if len == 56 {
            let mut data52 = Vec::with_capacity(52);
            for (group_idx, chunk) in clean.chunks(14).enumerate() {
                let expected = luhn_mod32_check_char(&chunk[0..13]);
                if chunk[13] != expected {
                    return Err(DeviceIdParseError::ChecksumMismatch {
                        first_group: group_idx * 2 + 1,
                    });
                }
                data52.extend_from_slice(&chunk[0..13]);
            }
            let mut raw = [0u8; 32];
            base32_decode_prefix(&data52, &mut raw).ok_or(DeviceIdParseError::DecodeFailed)?;
            return Ok(DeviceId(raw));
        }

        if len == 52 {
            let mut raw = [0u8; 32];
            base32_decode_prefix(&clean, &mut raw).ok_or(DeviceIdParseError::DecodeFailed)?;
            return Ok(DeviceId(raw));
        }

        // Shortened Device ID prefix (4..55 chars, e.g. 7-char Short ID)
        let capped_len = len.min(37);
        let mut data_chars = Vec::with_capacity(35);
        for (group_idx, chunk) in clean[..capped_len].chunks(14).enumerate() {
            if chunk.len() == 14 {
                let expected = luhn_mod32_check_char(&chunk[0..13]);
                if chunk[13] != expected {
                    return Err(DeviceIdParseError::ChecksumMismatch {
                        first_group: group_idx * 2 + 1,
                    });
                }
                data_chars.extend_from_slice(&chunk[0..13]);
            } else {
                data_chars.extend_from_slice(chunk);
            }
        }

        let mut raw = [0u8; 32];
        base32_decode_prefix(&data_chars, &mut raw[0..22])
            .ok_or(DeviceIdParseError::DecodeFailed)?;
        raw[23] = capped_len as u8;
        raw[24..32].copy_from_slice(SHORT_ID_SENTINEL);
        Ok(DeviceId(raw))
    }
}
