//! Syncthing Local Discovery v4 `Announce` packet codec (UDP port 21027).
//! Specification: https://docs.syncthing.net/specs/localdisco-v4.html
//!
//! UDP packet format:
//! - `0..4`: Magic number `0x2EA7D90B` (Big-Endian)
//! - `4..`: Protobuf message `Announce`:
//!   - `bytes id = 1` (32 bytes: SHA-256 of device TLS certificate)
//!   - `repeated string addresses = 2` (e.g. `["tcp://0.0.0.0:22000"]`)
//!   - `int64 instance_id = 3` (random instance identifier generated at startup)

use std::io;

use crate::device_id::DeviceId;
use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

/// Official UDP port for Syncthing Local Discovery v4
pub const DISCOVERY_PORT: u16 = 21027;

/// Default IPv4 broadcast address
pub const DISCOVERY_BROADCAST_ADDR: &str = "255.255.255.255:21027";

/// Syncthing magic number (`0x2EA7D90B`)
pub const DISCOVERY_MAGIC: u32 = 0x2E_A7_D9_0B;

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
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing `id` field (32 bytes) in Announce",
            )
        })?;

        Ok(Self {
            id: DeviceId(id),
            addresses,
            instance_id,
        })
    }
}
