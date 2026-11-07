//! Implementation of Syncthing's **Block Exchange Protocol v1 (BEP/1.0)**.
//! Specification: https://docs.syncthing.net/specs/bep-v1.html
//!
//! Covers:
//! - Pre-authentication `Hello` handshake
//! - Frame `Header` (`MessageType`, `MessageCompression`) + LZ4 decompression
//! - `ClusterConfig` messages (with `index_id` and `max_sequence` for bidirectional sync)
//! - `Index` / `IndexUpdate` messages (full encoding/decoding with BEP `Vector`/`Counter`)
//! - Bidirectional `Request` / `Response` messages (block download & upload)
//! - `Ping` and `Close` messages

use std::io::{self, Read, Write};

use crate::disco::{DeviceId, DISCOVERY_MAGIC};
use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

/// BEP magic number (`0x2EA7D90B`)
pub const BEP_MAGIC: u32 = DISCOVERY_MAGIC;

/// Default TCP port for Syncthing BEP protocol
pub const BEP_DEFAULT_PORT: u16 = 22000;

/// Standard block size for Syncthing BEP (128 KiB)
pub const BEP_BLOCK_SIZE: usize = 128 * 1024;

/// BEP/1.0 message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BepMessageType {
    ClusterConfig = 0,
    Index = 1,
    IndexUpdate = 2,
    Request = 3,
    Response = 4,
    DownloadProgress = 5,
    Ping = 6,
    Close = 7,
    Unknown(u8),
}

impl From<u64> for BepMessageType {
    fn from(v: u64) -> Self {
        match v {
            0 => Self::ClusterConfig,
            1 => Self::Index,
            2 => Self::IndexUpdate,
            3 => Self::Request,
            4 => Self::Response,
            5 => Self::DownloadProgress,
            6 => Self::Ping,
            7 => Self::Close,
            other => Self::Unknown(other as u8),
        }
    }
}

impl BepMessageType {
    pub fn as_u64(self) -> u64 {
        match self {
            Self::ClusterConfig => 0,
            Self::Index => 1,
            Self::IndexUpdate => 2,
            Self::Request => 3,
            Self::Response => 4,
            Self::DownloadProgress => 5,
            Self::Ping => 6,
            Self::Close => 7,
            Self::Unknown(v) => v as u64,
        }
    }
}

/// Compression mode of a BEP frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BepCompression {
    None = 0,
    Lz4 = 1,
}

/// Protobuf header (`Header`) preceding each post-`Hello` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BepHeader {
    pub msg_type: BepMessageType,
    pub compression: BepCompression,
}

impl BepHeader {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8);
        if self.msg_type.as_u64() != 0 {
            encode_varint_field(&mut buf, 1, self.msg_type.as_u64());
        }
        if self.compression == BepCompression::Lz4 {
            encode_varint_field(&mut buf, 2, 1);
        }
        buf
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut msg_type = BepMessageType::ClusterConfig;
        let mut compression = BepCompression::None;

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            if wire_type == 0 {
                let (v, _) = decode_varint(data)?;
                match field_num {
                    1 => msg_type = BepMessageType::from(v),
                    2 => {
                        compression = if v == 1 {
                            BepCompression::Lz4
                        } else {
                            BepCompression::None
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(Self {
            msg_type,
            compression,
        })
    }
}

/// `Hello` message exchanged at the start of a BEP connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepHello {
    pub device_name: String,
    pub client_name: String,
    pub client_version: String,
}

impl BepHello {
    pub fn new_vita_default() -> Self {
        Self {
            device_name: "PS-Vita".to_owned(),
            client_name: "vita-syncthing".to_owned(),
            client_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    pub fn encode_frame(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(64);
        if !self.device_name.is_empty() {
            encode_bytes_field(&mut payload, 1, self.device_name.as_bytes());
        }
        if !self.client_name.is_empty() {
            encode_bytes_field(&mut payload, 2, self.client_name.as_bytes());
        }
        if !self.client_version.is_empty() {
            encode_bytes_field(&mut payload, 3, self.client_version.as_bytes());
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

        for field_res in WireIter::new(payload) {
            let (field_num, wire_type, data) = field_res?;
            if wire_type == 2 {
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
        }

        Ok(Self {
            device_name,
            client_name,
            client_version,
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
        let out_frame = self.encode_frame();
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

/// Shared folder in `ClusterConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepFolder {
    pub id: String,
    pub label: String,
    pub device_ids: Vec<DeviceId>,
}

/// Folder index state metadata for `ClusterConfig`.
#[derive(Debug, Clone, Copy)]
pub struct FolderIndexState {
    pub index_id: u64,
    pub max_sequence: i64,
}

/// `ClusterConfig` message (type 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepClusterConfig {
    pub folders: Vec<BepFolder>,
}

impl BepClusterConfig {
    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut folders = Vec::new();
        for item in WireIter::new(payload) {
            let (field_num, wire_type, data) = item?;
            if field_num == 1 && wire_type == 2 {
                folders.push(Self::decode_folder(data)?);
            }
        }
        Ok(Self { folders })
    }

    fn decode_folder(bytes: &[u8]) -> io::Result<BepFolder> {
        let mut id = String::new();
        let mut label = String::new();
        let mut device_ids = Vec::new();

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            if wire_type == 2 {
                match field_num {
                    1 => id = String::from_utf8_lossy(data).into_owned(),
                    2 => label = String::from_utf8_lossy(data).into_owned(),
                    16 => {
                        for dev_item in WireIter::new(data) {
                            let (df, dw, ddata) = dev_item?;
                            if df == 1 && dw == 2 && ddata.len() == 32 {
                                let mut raw = [0u8; 32];
                                raw.copy_from_slice(ddata);
                                device_ids.push(DeviceId(raw));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(BepFolder {
            id,
            label,
            device_ids,
        })
    }

    /// Encodes a `ClusterConfig` advertising our shared folders, `index_id`, and `max_sequence`.
    pub fn encode_for_peers(
        folders: &[(BepFolder, FolderIndexState)],
        own_id: DeviceId,
        peer_id: DeviceId,
    ) -> Vec<u8> {
        let mut out = Vec::new();

        for (folder, state) in folders {
            let mut folder_buf = Vec::new();
            encode_bytes_field(&mut folder_buf, 1, folder.id.as_bytes());
            if !folder.label.is_empty() {
                encode_bytes_field(&mut folder_buf, 2, folder.label.as_bytes());
            }

            // Device entry for PS Vita (with index_id and max_sequence from local index)
            // BEP v1 Device fields: 1=id, 2=name, 3=addresses, 4=compression, 6=max_sequence, 8=index_id
            {
                let mut dev_buf = Vec::new();
                encode_bytes_field(&mut dev_buf, 1, &own_id.0);
                encode_bytes_field(&mut dev_buf, 2, b"PS-Vita");
                encode_bytes_field(&mut dev_buf, 3, b"dynamic");
                encode_varint_field(&mut dev_buf, 4, 1); // COMPRESSION_NEVER
                if state.max_sequence > 0 {
                    encode_varint_field(&mut dev_buf, 6, state.max_sequence as u64);
                }
                if state.index_id != 0 {
                    encode_varint_field(&mut dev_buf, 8, state.index_id);
                }
                encode_bytes_field(&mut folder_buf, 16, &dev_buf);
            }

            // Device entry for remote peer
            {
                let mut dev_buf = Vec::new();
                encode_bytes_field(&mut dev_buf, 1, &peer_id.0);
                encode_bytes_field(&mut dev_buf, 3, b"dynamic");
                encode_varint_field(&mut dev_buf, 4, 1); // COMPRESSION_NEVER
                encode_bytes_field(&mut folder_buf, 16, &dev_buf);
            }

            encode_bytes_field(&mut out, 1, &folder_buf);
        }

        out
    }
}

/// BEP version vector counter (`Counter { uint64 id = 1; uint64 value = 2; }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BepVersionCounter {
    pub short_device_id: u64,
    pub value: u64,
}

/// Information about a file block in `Index` / `IndexUpdate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepBlockInfo {
    pub offset: i64,
    pub size: i32,
    pub hash: Vec<u8>,
}

/// Information about a file in `Index` / `IndexUpdate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepFileInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified_s: i64,
    pub deleted: bool,
    pub invalid: bool,
    pub sequence: i64,
    pub versions: Vec<BepVersionCounter>,
    pub blocks: Vec<BepBlockInfo>,
}

/// `Index` (type 1) or `IndexUpdate` (type 2) message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepIndex {
    pub folder: String,
    pub files: Vec<BepFileInfo>,
}

impl BepIndex {
    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut folder = String::new();
        let mut files = Vec::new();

        for item in WireIter::new(payload) {
            let (field_num, wire_type, data) = item?;
            if wire_type == 2 {
                match field_num {
                    1 => folder = String::from_utf8_lossy(data).into_owned(),
                    2 => files.push(Self::decode_file_info(data)?),
                    _ => {}
                }
            }
        }

        Ok(Self { folder, files })
    }

    /// Encodes a full Protobuf `Index` or `IndexUpdate` message advertising local Vita files.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_bytes_field(&mut buf, 1, self.folder.as_bytes());

        for f in &self.files {
            let mut fbuf = Vec::new();
            encode_bytes_field(&mut fbuf, 1, f.name.as_bytes());
            if f.is_dir {
                encode_varint_field(&mut fbuf, 2, 1);
                encode_varint_field(&mut fbuf, 4, 0o755);
            } else {
                if f.size != 0 {
                    encode_varint_field(&mut fbuf, 3, f.size as u64);
                }
                // field 4: permissions (0o644 = 420)
                encode_varint_field(&mut fbuf, 4, 0o644);
            }
            if f.modified_s != 0 {
                encode_varint_field(&mut fbuf, 5, f.modified_s as u64);
            }
            if f.deleted {
                encode_varint_field(&mut fbuf, 6, 1);
            }
            if f.invalid {
                encode_varint_field(&mut fbuf, 7, 1);
            }
            // field 8: no_permissions = true (ux0: filesystem has no POSIX permissions)
            encode_varint_field(&mut fbuf, 8, 1);

            // field 9: Vector version
            if !f.versions.is_empty() {
                let mut vec_buf = Vec::new();
                for c in &f.versions {
                    let mut cbuf = Vec::new();
                    encode_varint_field(&mut cbuf, 1, c.short_device_id);
                    encode_varint_field(&mut cbuf, 2, c.value);
                    encode_bytes_field(&mut vec_buf, 1, &cbuf);
                }
                encode_bytes_field(&mut fbuf, 9, &vec_buf);
            }

            if f.sequence > 0 {
                encode_varint_field(&mut fbuf, 10, f.sequence as u64);
            }
            if let Some(last_ver) = f.versions.last() {
                // field 12: uint64 modified_by
                encode_varint_field(&mut fbuf, 12, last_ver.short_device_id);
            }
            if !f.is_dir {
                // field 13: int32 block_size = 128 KiB
                encode_varint_field(&mut fbuf, 13, BEP_BLOCK_SIZE as u64);

                for b in &f.blocks {
                    let mut bbuf = Vec::new();
                    if b.offset != 0 {
                        encode_varint_field(&mut bbuf, 1, b.offset as u64);
                    }
                    if b.size != 0 {
                        encode_varint_field(&mut bbuf, 2, b.size as u64);
                    }
                    if !b.hash.is_empty() {
                        encode_bytes_field(&mut bbuf, 3, &b.hash);
                    }
                    encode_bytes_field(&mut fbuf, 16, &bbuf);
                }
            }

            encode_bytes_field(&mut buf, 2, &fbuf);
        }

        buf
    }

    fn decode_file_info(bytes: &[u8]) -> io::Result<BepFileInfo> {
        let mut name = String::new();
        let mut is_dir = false;
        let mut size = 0i64;
        let mut modified_s = 0i64;
        let mut deleted = false;
        let mut invalid = false;
        let mut sequence = 0i64;
        let mut versions = Vec::new();
        let mut blocks = Vec::new();

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            match (field_num, wire_type) {
                (1, 2) => name = String::from_utf8_lossy(data).into_owned(),
                (2, 0) => {
                    let (v, _) = decode_varint(data)?;
                    is_dir = v == 1;
                }
                (3, 0) => {
                    let (v, _) = decode_varint(data)?;
                    size = v as i64;
                }
                (5, 0) => {
                    let (v, _) = decode_varint(data)?;
                    modified_s = v as i64;
                }
                (6, 0) => {
                    let (v, _) = decode_varint(data)?;
                    deleted = v != 0;
                }
                (7, 0) => {
                    let (v, _) = decode_varint(data)?;
                    invalid = v != 0;
                }
                (9, 2) => {
                    for c_item in WireIter::new(data) {
                        let (cf, cw, cdata) = c_item?;
                        if cf == 1 && cw == 2 {
                            let mut short_id = 0u64;
                            let mut val = 0u64;
                            for kv in WireIter::new(cdata) {
                                let (kf, kw, kdata) = kv?;
                                if kw == 0 {
                                    let (v, _) = decode_varint(kdata)?;
                                    if kf == 1 {
                                        short_id = v;
                                    } else if kf == 2 {
                                        val = v;
                                    }
                                }
                            }
                            versions.push(BepVersionCounter {
                                short_device_id: short_id,
                                value: val,
                            });
                        }
                    }
                }
                (10, 0) => {
                    let (v, _) = decode_varint(data)?;
                    sequence = v as i64;
                }
                (16, 2) => blocks.push(Self::decode_block_info(data)?),
                _ => {}
            }
        }

        Ok(BepFileInfo {
            name,
            is_dir,
            size,
            modified_s,
            deleted,
            invalid,
            sequence,
            versions,
            blocks,
        })
    }

    fn decode_block_info(bytes: &[u8]) -> io::Result<BepBlockInfo> {
        let mut offset = 0i64;
        let mut size = 0i32;
        let mut hash = Vec::new();

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            match (field_num, wire_type) {
                (1, 0) => {
                    let (v, _) = decode_varint(data)?;
                    offset = v as i64;
                }
                (2, 0) => {
                    let (v, _) = decode_varint(data)?;
                    size = v as i32;
                }
                (3, 2) => hash = data.to_vec(),
                _ => {}
            }
        }

        Ok(BepBlockInfo { offset, size, hash })
    }
}

/// `Request` message (type 3) to request or serve a file block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepRequest {
    pub id: i32,
    pub folder: String,
    pub name: String,
    pub offset: i64,
    pub size: i32,
    pub hash: Vec<u8>,
}

impl BepRequest {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        encode_varint_field(&mut buf, 1, self.id as u64);
        encode_bytes_field(&mut buf, 2, self.folder.as_bytes());
        encode_bytes_field(&mut buf, 3, self.name.as_bytes());
        if self.offset != 0 {
            encode_varint_field(&mut buf, 4, self.offset as u64);
        }
        if self.size != 0 {
            encode_varint_field(&mut buf, 5, self.size as u64);
        }
        if !self.hash.is_empty() {
            encode_bytes_field(&mut buf, 6, &self.hash);
        }
        buf
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut id = 0i32;
        let mut folder = String::new();
        let mut name = String::new();
        let mut offset = 0i64;
        let mut size = 0i32;
        let mut hash = Vec::new();

        for item in WireIter::new(payload) {
            let (field_num, wire_type, slice) = item?;
            match (field_num, wire_type) {
                (1, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    id = v as i32;
                }
                (2, 2) => folder = String::from_utf8_lossy(slice).into_owned(),
                (3, 2) => name = String::from_utf8_lossy(slice).into_owned(),
                (4, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    offset = v as i64;
                }
                (5, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    size = v as i32;
                }
                (6, 2) => hash = slice.to_vec(),
                _ => {}
            }
        }

        Ok(Self {
            id,
            folder,
            name,
            offset,
            size,
            hash,
        })
    }
}

/// `Response` message (type 4) containing block data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepResponse {
    pub id: i32,
    pub data: Vec<u8>,
    pub code: i32,
}

impl BepResponse {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.data.len());
        encode_varint_field(&mut buf, 1, self.id as u64);
        if !self.data.is_empty() {
            encode_bytes_field(&mut buf, 2, &self.data);
        }
        if self.code != 0 {
            encode_varint_field(&mut buf, 3, self.code as u64);
        }
        buf
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut id = 0i32;
        let mut data = Vec::new();
        let mut code = 0i32;

        for item in WireIter::new(payload) {
            let (field_num, wire_type, slice) = item?;
            match (field_num, wire_type) {
                (1, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    id = v as i32;
                }
                (2, 2) => data = slice.to_vec(),
                (3, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    code = v as i32;
                }
                _ => {}
            }
        }

        Ok(Self { id, data, code })
    }
}

/// `Close` message (type 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepClose {
    pub reason: String,
}

impl BepClose {
    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut reason = String::new();
        for item in WireIter::new(payload) {
            let (field_num, wire_type, slice) = item?;
            if field_num == 1 && wire_type == 2 {
                reason = String::from_utf8_lossy(slice).into_owned();
            }
        }
        Ok(Self { reason })
    }
}

/// Writes an uncompressed post-`Hello` BEP message to a stream.
pub fn write_bep_message<W: Write>(
    stream: &mut W,
    msg_type: BepMessageType,
    payload: &[u8],
) -> io::Result<()> {
    let hdr = BepHeader {
        msg_type,
        compression: BepCompression::None,
    };
    let hdr_bytes = hdr.encode();
    let hdr_len = hdr_bytes.len() as u16;
    let msg_len = payload.len() as u32;

    stream.write_all(&hdr_len.to_be_bytes())?;
    stream.write_all(&hdr_bytes)?;
    stream.write_all(&msg_len.to_be_bytes())?;
    stream.write_all(payload)?;
    stream.flush()
}

/// Reads and decompresses (if LZ4) the next post-`Hello` BEP message from a stream.
pub fn read_bep_message<R: Read>(stream: &mut R) -> io::Result<(BepMessageType, Vec<u8>)> {
    let mut hdr_len_buf = [0u8; 2];
    stream.read_exact(&mut hdr_len_buf)?;
    let hdr_len = u16::from_be_bytes(hdr_len_buf) as usize;

    let mut hdr_buf = vec![0u8; hdr_len];
    stream.read_exact(&mut hdr_buf)?;
    let header = BepHeader::decode(&hdr_buf)?;

    let mut msg_len_buf = [0u8; 4];
    stream.read_exact(&mut msg_len_buf)?;
    let msg_len = u32::from_be_bytes(msg_len_buf) as usize;

    if msg_len > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("BEP frame exceeds Vita RAM safeguard limit: {} bytes", msg_len),
        ));
    }

    let mut raw_msg = vec![0u8; msg_len];
    stream.read_exact(&mut raw_msg)?;

    let payload = match header.compression {
        BepCompression::None => raw_msg,
        BepCompression::Lz4 => decompress_bep_lz4(&raw_msg)?,
    };

    Ok((header.msg_type, payload))
}

/// Decompresses an LZ4 block in Syncthing BEP format (`[u32 BE uncompressed_len][LZ4 block]`).
pub fn decompress_bep_lz4(input: &[u8]) -> io::Result<Vec<u8>> {
    if input.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "BEP LZ4 payload too short (< 4 bytes)",
        ));
    }
    let uncompressed_len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if uncompressed_len > 32 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Excessive LZ4 uncompressed size: {}", uncompressed_len),
        ));
    }

    let src = &input[4..];
    let mut out = Vec::with_capacity(uncompressed_len);
    let mut pos = 0usize;

    while pos < src.len() {
        let token = src[pos];
        pos += 1;

        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            loop {
                if pos >= src.len() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "LZ4 lit_len EOF"));
                }
                let b = src[pos] as usize;
                pos += 1;
                lit_len += b;
                if b != 255 {
                    break;
                }
            }
        }

        if pos + lit_len > src.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "LZ4 literals EOF"));
        }
        out.extend_from_slice(&src[pos..pos + lit_len]);
        pos += lit_len;

        if pos >= src.len() {
            break;
        }

        if pos + 2 > src.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "LZ4 offset EOF"));
        }
        let offset = u16::from_le_bytes([src[pos], src[pos + 1]]) as usize;
        pos += 2;
        if offset == 0 || offset > out.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid LZ4 offset: {offset} (out.len={})", out.len()),
            ));
        }

        let mut match_len = ((token & 0x0F) as usize) + 4;
        if (token & 0x0F) == 15 {
            loop {
                if pos >= src.len() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "LZ4 match_len EOF"));
                }
                let b = src[pos] as usize;
                pos += 1;
                match_len += b;
                if b != 255 {
                    break;
                }
            }
        }

        let match_start = out.len() - offset;
        for i in 0..match_len {
            let b = out[match_start + i];
            out.push(b);
        }
    }

    Ok(out)
}
