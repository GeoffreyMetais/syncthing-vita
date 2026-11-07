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
