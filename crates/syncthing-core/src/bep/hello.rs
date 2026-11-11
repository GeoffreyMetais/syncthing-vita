//! Pre-authentication `Hello` handshake message.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{BEP_DEFAULT_NUM_CONNECTIONS, BEP_MAGIC};
use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

static HELLO_TS_COUNTER: AtomicI64 = AtomicI64::new(1);

fn next_hello_timestamp_ns() -> i64 {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(1_700_000_000_000_000_000);
    let seq = HELLO_TS_COUNTER.fetch_add(1, Ordering::Relaxed);
    now_ns.saturating_add(seq)
}

/// `Hello` message exchanged at the start of a BEP connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepHello {
    pub device_name: String,
    pub client_name: String,
    pub client_version: String,
    pub num_connections: i32,
    pub timestamp: i64,
}

impl BepHello {
    pub fn new_vita_default() -> Self {
        Self {
            device_name: "PS-Vita".to_owned(),
            client_name: "vita-syncthing".to_owned(),
            client_version: env!("CARGO_PKG_VERSION").to_owned(),
            num_connections: BEP_DEFAULT_NUM_CONNECTIONS,
            timestamp: 0,
        }
    }

    pub fn encode_frame(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(80);
        if !self.device_name.is_empty() {
            encode_bytes_field(&mut payload, 1, self.device_name.as_bytes());
        }
        if !self.client_name.is_empty() {
            encode_bytes_field(&mut payload, 2, self.client_name.as_bytes());
        }
        if !self.client_version.is_empty() {
            encode_bytes_field(&mut payload, 3, self.client_version.as_bytes());
        }
        if self.num_connections > 0 {
            encode_varint_field(&mut payload, 4, self.num_connections as u64);
        }
        if self.timestamp > 0 {
            encode_varint_field(&mut payload, 5, self.timestamp as u64);
        }

        let len = payload.len() as u16;
        let mut frame = Vec::with_capacity(6 + payload.len());
        frame.extend_from_slice(&BEP_MAGIC.to_be_bytes());
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(&payload);
        frame
    }

    pub fn decode_payload(payload: &[u8]) -> io::Result<Self> {
        let mut device_name = String::new();
        let mut client_name = String::new();
        let mut client_version = String::new();
        let mut num_connections = 0i32;
        let mut timestamp = 0i64;

        for field_res in WireIter::new(payload) {
            let (field_num, wire_type, data) = field_res?;
            match (field_num, wire_type) {
                (1, 2) | (2, 2) | (3, 2) => {
                    let s = std::str::from_utf8(data).map_err(|e| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("Invalid UTF-8: {e}"))
                    })?;
                    match field_num {
                        1 => device_name = s.to_owned(),
                        2 => client_name = s.to_owned(),
                        3 => client_version = s.to_owned(),
                        _ => {}
                    }
                }
                (4, 0) => {
                    let (v, _) = decode_varint(data)?;
                    num_connections = v as i32;
                }
                (5, 0) => {
                    let (v, _) = decode_varint(data)?;
                    timestamp = v as i64;
                }
                _ => {}
            }
        }

        Ok(Self {
            device_name,
            client_name,
            client_version,
            num_connections,
            timestamp,
        })
    }

    pub fn decode_frame(frame: &[u8]) -> io::Result<Self> {
        if frame.len() < 6 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "BEP Hello frame too short (< 6 bytes)",
            ));
        }
        let magic = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
        if magic != BEP_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid BEP Hello magic: 0x{:08X}", magic),
            ));
        }
        let len = u16::from_be_bytes([frame[4], frame[5]]) as usize;
        if frame.len() < 6 + len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Incomplete BEP Hello payload",
            ));
        }
        Self::decode_payload(&frame[6..6 + len])
    }

    pub fn exchange<S: Read + Write>(&self, stream: &mut S) -> io::Result<Self> {
        let mut outgoing = self.clone();
        if outgoing.timestamp == 0 {
            outgoing.timestamp = next_hello_timestamp_ns();
        }
        let out_frame = outgoing.encode_frame();
        stream.write_all(&out_frame)?;
        stream.flush()?;

        let mut header = [0u8; 6];
        stream.read_exact(&mut header)?;
        let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if magic != BEP_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid peer BEP magic: 0x{:08X}", magic),
            ));
        }
        let len = u16::from_be_bytes([header[4], header[5]]) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload)?;
        Self::decode_payload(&payload)
    }
}
