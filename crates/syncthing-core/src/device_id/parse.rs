//! Parsing of user-entered Syncthing Device IDs (on-screen keyboard / manual pairing).

use super::base32::{base32_decode_prefix, luhn_mod32_check_char, BASE32_ALPHABET};
use super::{DeviceId, SHORT_ID_SENTINEL};

impl DeviceId {
    /// Parses a user-entered Syncthing Device ID:
    /// - Full 56-character Syncthing ID (with or without hyphens, validated via Luhn mod 32)
    /// - Full 52-character raw Base32 ID (without Luhn check characters)
    /// - Full 64-character hexadecimal ID
    /// - Shortened Syncthing ID prefix (`4..51` Base32 characters, e.g. 7-character Short ID `MFZWI3D`)
    pub fn parse_user_id(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Device ID cannot be empty".to_owned());
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
                return Err(format!("Invalid character '{}' (use Base32 A-Z, 2-7)", ch));
            }
            clean.push(up as u8);
        }

        let len = clean.len();
        if len < 4 {
            return Err("Enter at least 7 characters (Short ID) or full 56-char ID".to_owned());
        }
        if len > 56 {
            return Err(format!("Device ID is too long ({} chars, max 56)", len));
        }

        if len == 56 {
            let mut data52 = Vec::with_capacity(52);
            for (group_idx, chunk) in clean.chunks(14).enumerate() {
                let expected = luhn_mod32_check_char(&chunk[0..13]);
                if chunk[13] != expected {
                    return Err(format!(
                        "Typo detected in group {}-{} (check digit mismatch)",
                        group_idx * 2 + 1,
                        group_idx * 2 + 2
                    ));
                }
                data52.extend_from_slice(&chunk[0..13]);
            }
            let mut raw = [0u8; 32];
            base32_decode_prefix(&data52, &mut raw)
                .ok_or_else(|| "Base32 decode failed".to_owned())?;
            return Ok(DeviceId(raw));
        }

        if len == 52 {
            let mut raw = [0u8; 32];
            base32_decode_prefix(&clean, &mut raw)
                .ok_or_else(|| "Base32 decode failed".to_owned())?;
            return Ok(DeviceId(raw));
        }

        // Shortened Device ID prefix (4..55 chars, e.g. 7-char Short ID)
        let capped_len = len.min(37);
        let mut data_chars = Vec::with_capacity(35);
        for (group_idx, chunk) in clean[..capped_len].chunks(14).enumerate() {
            if chunk.len() == 14 {
                let expected = luhn_mod32_check_char(&chunk[0..13]);
                if chunk[13] != expected {
                    return Err(format!(
                        "Typo detected in group {}-{} (check digit mismatch)",
                        group_idx * 2 + 1,
                        group_idx * 2 + 2
                    ));
                }
                data_chars.extend_from_slice(&chunk[0..13]);
            } else {
                data_chars.extend_from_slice(chunk);
            }
        }

        let mut raw = [0u8; 32];
        base32_decode_prefix(&data_chars, &mut raw[0..22])
            .ok_or_else(|| "Base32 prefix decode failed".to_owned())?;
        raw[23] = capped_len as u8;
        raw[24..32].copy_from_slice(SHORT_ID_SENTINEL);
        Ok(DeviceId(raw))
    }
}
