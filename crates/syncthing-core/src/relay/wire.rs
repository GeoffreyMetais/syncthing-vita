//! XDR wire encoding and decoding for Syncthing Relay Protocol v1 (`0x9E79BC40`).

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::device_id::DeviceId;

/// Magic value preceding every Relay Protocol v1 header.
pub const RELAY_MAGIC: u32 = 0x9E79_BC40;
/// ALPN protocol name used in Relay Protocol mode (`TLS`).
pub const RELAY_ALPN: &[u8] = b"bep-relay";

/// Relay Protocol v1 message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum RelayMsgType {
    Ping = 0,
    Pong = 1,
    JoinRelayRequest = 2,
    JoinSessionRequest = 3,
    Response = 4,
    ConnectRequest = 5,
    SessionInvitation = 6,
    RelayFull = 7,
}

impl RelayMsgType {
    pub fn from_i32(v: i32) -> io::Result<Self> {
        match v {
            0 => Ok(Self::Ping),
            1 => Ok(Self::Pong),
            2 => Ok(Self::JoinRelayRequest),
            3 => Ok(Self::JoinSessionRequest),
            4 => Ok(Self::Response),
            5 => Ok(Self::ConnectRequest),
            6 => Ok(Self::SessionInvitation),
            7 => Ok(Self::RelayFull),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown relay message type: {v}"),
            )),
        }
    }
}

/// Decoded `Response` message (type 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayResponse {
    pub code: i32,
    pub message: String,
}

/// Decoded `SessionInvitation` message (type 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInvitation {
    pub from: Vec<u8>,
    pub key: Vec<u8>,
    pub address: Vec<u8>,
    pub port: u16,
    pub server_socket: bool,
}

impl SessionInvitation {
    /// Resolves the relay session IP, falling back to `fallback_ip` when the invitation omits
    /// the address or specifies an unspecified (`0.0.0.0` / `::`) address.
    pub fn session_ip(&self, fallback_ip: IpAddr) -> IpAddr {
        match self.address.len() {
            4 => {
                let ip = Ipv4Addr::new(
                    self.address[0],
                    self.address[1],
                    self.address[2],
                    self.address[3],
                );
                if ip.is_unspecified() {
                    fallback_ip
                } else {
                    IpAddr::V4(ip)
                }
            }
            16 => {
                let mut octets = [0u8; 16];
                octets.copy_from_slice(&self.address);
                let ip = Ipv6Addr::from(octets);
                if let Some(v4) = ip.to_ipv4_mapped() {
                    if v4.is_unspecified() {
                        fallback_ip
                    } else {
                        IpAddr::V4(v4)
                    }
                } else if ip.is_unspecified() {
                    fallback_ip
                } else {
                    IpAddr::V6(ip)
                }
            }
            _ => fallback_ip,
        }
    }
}

/// Decoded Relay Protocol v1 message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayMessage {
    Ping,
    Pong,
    Response(RelayResponse),
    SessionInvitation(SessionInvitation),
    RelayFull,
}

/// Appends XDR variable-length opaque bytes (4-byte big-endian length + data + 0..3 zero pad).
fn push_xdr_opaque(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(data);
    let pad = (4 - (data.len() % 4)) % 4;
    buf.extend(std::iter::repeat_n(0u8, pad));
}

/// Wraps `payload` in a 12-byte Relay Protocol v1 header.
fn frame_message(msg_type: RelayMsgType, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(&RELAY_MAGIC.to_be_bytes());
    out.extend_from_slice(&(msg_type as i32).to_be_bytes());
    out.extend_from_slice(&(payload.len() as i32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Encodes a `ConnectRequest` (type 5) for `target_id`.
pub fn encode_connect_request(target_id: &DeviceId) -> Vec<u8> {
    let mut payload = Vec::with_capacity(36);
    push_xdr_opaque(&mut payload, &target_id.0);
    frame_message(RelayMsgType::ConnectRequest, &payload)
}

/// Encodes a `JoinSessionRequest` (type 3) for `key`.
pub fn encode_join_session_request(key: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(4 + key.len() + 3);
    push_xdr_opaque(&mut payload, key);
    frame_message(RelayMsgType::JoinSessionRequest, &payload)
}

/// Encodes a `JoinRelayRequest` (type 2) with an empty token: joins the relay permanently to
/// receive `SessionInvitation`s for incoming connections.
pub fn encode_join_relay_request() -> Vec<u8> {
    let mut payload = Vec::with_capacity(4);
    push_xdr_opaque(&mut payload, b"");
    frame_message(RelayMsgType::JoinRelayRequest, &payload)
}

/// Encodes a `Ping` (type 0).
pub fn encode_ping() -> Vec<u8> {
    frame_message(RelayMsgType::Ping, &[])
}

/// Encodes a `Pong` (type 1), the answer to a relay `Ping`.
pub fn encode_pong() -> Vec<u8> {
    frame_message(RelayMsgType::Pong, &[])
}

/// Reads one complete relay message from `reader`.
pub fn read_relay_message<R: io::Read>(reader: &mut R) -> io::Result<RelayMessage> {
    let mut header = [0u8; 12];
    reader.read_exact(&mut header)?;
    read_relay_message_after_header(reader, header)
}

/// Reads the payload of a relay message whose 12-byte `header` was already read.
pub fn read_relay_message_after_header<R: io::Read>(
    reader: &mut R,
    header: [u8; 12],
) -> io::Result<RelayMessage> {
    let (msg_type, len) = decode_header(header)?;
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    decode_payload(msg_type, &payload)
}

/// Parses a 12-byte relay header into `(msg_type, payload_len)`.
pub fn decode_header(header: [u8; 12]) -> io::Result<(RelayMsgType, usize)> {
    let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    if magic != RELAY_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid relay magic: 0x{magic:08x}"),
        ));
    }
    let msg_type = RelayMsgType::from_i32(i32::from_be_bytes([
        header[4], header[5], header[6], header[7],
    ]))?;
    let len = i32::from_be_bytes([header[8], header[9], header[10], header[11]]);
    if !(0..=65536).contains(&len) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid relay payload length: {len}"),
        ));
    }
    Ok((msg_type, len as usize))
}

fn read_u32(buf: &[u8], pos: &mut usize) -> io::Result<u32> {
    if *pos + 4 > buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Truncated relay XDR u32",
        ));
    }
    let v = u32::from_be_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], buf[*pos + 3]]);
    *pos += 4;
    Ok(v)
}

fn read_xdr_opaque(buf: &[u8], pos: &mut usize) -> io::Result<Vec<u8>> {
    let len = read_u32(buf, pos)? as usize;
    // `len` comes from the relay: the addition must not wrap on 32-bit targets.
    let end = pos
        .checked_add(len)
        .filter(|end| *end <= buf.len())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "Truncated relay XDR opaque")
        })?;
    let out = buf[*pos..end].to_vec();
    let pad = (4 - (len % 4)) % 4;
    *pos = (*pos + len + pad).min(buf.len());
    Ok(out)
}

/// Decodes a relay message given its parsed `msg_type` and `payload`.
pub fn decode_payload(msg_type: RelayMsgType, payload: &[u8]) -> io::Result<RelayMessage> {
    let mut pos = 0;
    match msg_type {
        RelayMsgType::Ping => Ok(RelayMessage::Ping),
        RelayMsgType::Pong => Ok(RelayMessage::Pong),
        RelayMsgType::RelayFull => Ok(RelayMessage::RelayFull),
        RelayMsgType::Response => {
            let code = read_u32(payload, &mut pos)? as i32;
            let msg_bytes = read_xdr_opaque(payload, &mut pos)?;
            let message = String::from_utf8_lossy(&msg_bytes).into_owned();
            Ok(RelayMessage::Response(RelayResponse { code, message }))
        }
        RelayMsgType::SessionInvitation => {
            let from = read_xdr_opaque(payload, &mut pos)?;
            let key = read_xdr_opaque(payload, &mut pos)?;
            let address = read_xdr_opaque(payload, &mut pos)?;
            let port_u32 = read_u32(payload, &mut pos)?;
            let port = u16::try_from(port_u32).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid port: {port_u32}"),
                )
            })?;
            let server_socket = read_u32(payload, &mut pos)? != 0;
            Ok(RelayMessage::SessionInvitation(SessionInvitation {
                from,
                key,
                address,
                port,
                server_socket,
            }))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unexpected relay message type from server: {msg_type:?}"),
        )),
    }
}
