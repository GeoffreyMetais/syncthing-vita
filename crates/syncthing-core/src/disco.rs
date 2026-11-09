//! Implementation of the **Syncthing Local Discovery v4** protocol (UDP port 21027).
//! Specification: https://docs.syncthing.net/specs/localdisco-v4.html
//!
//! UDP packet format:
//! - `0..4`: Magic number `0x2EA7D90B` (Big-Endian)
//! - `4..`: Protobuf message `Announce`:
//!   - `bytes id = 1` (32 bytes: SHA-256 of device TLS certificate)
//!   - `repeated string addresses = 2` (e.g. `["tcp://0.0.0.0:22000"]`)
//!   - `int64 instance_id = 3` (random instance identifier generated at startup)

use std::fmt;
use std::io;

use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

/// Official UDP port for Syncthing Local Discovery v4
pub const DISCOVERY_PORT: u16 = 21027;

/// Default IPv4 broadcast address
pub const DISCOVERY_BROADCAST_ADDR: &str = "255.255.255.255:21027";

/// Syncthing magic number (`0x2EA7D90B`)
pub const DISCOVERY_MAGIC: u32 = 0x2E_A7_D9_0B;

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

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
                return Err(format!(
                    "Invalid character '{}' (use Base32 A-Z, 2-7)",
                    ch
                ));
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

/// Protobuf message `Announce` for Syncthing Local Discovery v4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announce {
    pub id: DeviceId,
    pub addresses: Vec<String>,
    pub instance_id: i64,
}

impl Announce {
    /// Serializes the complete packet: `[MAGIC (4 bytes BE)] + [Protobuf Announce]`.
    pub fn encode_packet(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(&DISCOVERY_MAGIC.to_be_bytes());

        // Field 1: bytes id
        encode_bytes_field(&mut buf, 1, &self.id.0);

        // Field 2: repeated string addresses
        for addr in &self.addresses {
            encode_bytes_field(&mut buf, 2, addr.as_bytes());
        }

        // Field 3: int64 instance_id
        if self.instance_id != 0 {
            encode_varint_field(&mut buf, 3, self.instance_id as u64);
        }

        buf
    }

    /// Deserializes a UDP packet `[MAGIC (4 bytes BE)] + [Protobuf Announce]`.
    pub fn decode_packet(packet: &[u8]) -> io::Result<Self> {
        if packet.len() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Packet too short (< 4 bytes)",
            ));
        }

        let magic = u32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]);
        if magic != DISCOVERY_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid magic header: 0x{:08X}", magic),
            ));
        }

        let mut id_bytes: Option<[u8; 32]> = None;
        let mut addresses = Vec::new();
        let mut instance_id: i64 = 0;

        for field_res in WireIter::new(&packet[4..]) {
            let (field_num, wire_type, payload) = field_res?;
            match (field_num, wire_type) {
                (1, 2) => {
                    if payload.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(payload);
                        id_bytes = Some(arr);
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Invalid DeviceId length: {} (expected 32)", payload.len()),
                        ));
                    }
                }
                (2, 2) => {
                    let s = std::str::from_utf8(payload).map_err(|e| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {e}"))
                    })?;
                    addresses.push(s.to_owned());
                }
                (3, 0) => {
                    let (val, _) = decode_varint(payload)?;
                    instance_id = val as i64;
                }
                _ => {
                    // Future fields ignored per Protobuf v3 spec
                }
            }
        }

        let id = id_bytes.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Missing `id` field (32 bytes) in Announce")
        })?;

        Ok(Self {
            id: DeviceId(id),
            addresses,
            instance_id,
        })
    }
}

/// Encodes 32 bytes into RFC 4648 Base32 without padding (52 characters for 256 bits).
fn base32_encode(data: &[u8; 32]) -> String {
    let mut out = String::with_capacity(52);
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;

    for &byte in data.iter() {
        buffer = (buffer << 8) | (byte as u32);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1F) as usize;
            out.push(BASE32_ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        out.push(BASE32_ALPHABET[idx] as char);
    }
    out
}

/// Decodes Base32 characters (`A-Z`, `2-7`) into `out` (MSB-first, 5 bits per character).
fn base32_decode_prefix(b32_chars: &[u8], out: &mut [u8]) -> Option<()> {
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;
    let mut out_idx: usize = 0;

    for &ch in b32_chars {
        let val = BASE32_ALPHABET.iter().position(|&b| b == ch)? as u32;
        buffer = (buffer << 5) | val;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            if out_idx < out.len() {
                out[out_idx] = ((buffer >> bits_left) & 0xFF) as u8;
                out_idx += 1;
            }
        }
    }
    if bits_left > 0 && out_idx < out.len() {
        out[out_idx] = ((buffer << (8 - bits_left)) & 0xFF) as u8;
    }
    Some(())
}

/// Computes the Luhn mod 32 check character used by Syncthing Device IDs.
fn luhn_mod32_check_char(chunk: &[u8]) -> u8 {
    let mut factor = 1usize;
    let mut sum = 0usize;
    let n = 32usize;

    for &ch in chunk {
        let code = BASE32_ALPHABET
            .iter()
            .position(|&b| b == ch)
            .unwrap_or(0);
        let mut addend = factor * code;
        factor = if factor == 2 { 1 } else { 2 };
        addend = (addend / n) + (addend % n);
        sum += addend;
    }

    let remainder = sum % n;
    let check_code = (n - remainder) % n;
    BASE32_ALPHABET[check_code]
}
