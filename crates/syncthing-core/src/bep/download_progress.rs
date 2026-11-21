//! `DownloadProgress` message (type 5): notifies peers about blocks available in temporary files.

use std::io;

use super::BepVersionCounter;
use crate::proto::{
    decode_varint, encode_bytes_field, encode_varint, encode_varint_field, WireIter,
};

/// Update kind in a [`BepFileDownloadProgressUpdate`].
///
/// Matches BEP v1 `FileDownloadProgressUpdateType`: `APPEND = 0`, `FORGET = 1`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DownloadProgressUpdateType {
    /// Advertises newly downloaded block indexes for the file's temporary copy.
    #[default]
    Update = 0,
    /// Retracts previously advertised temporary blocks (file transfer completed or failed).
    Forget = 1,
}

impl DownloadProgressUpdateType {
    /// Decodes a wire varint into [`DownloadProgressUpdateType`].
    pub fn from_wire(v: u64) -> Self {
        match v {
            1 => Self::Forget,
            _ => Self::Update,
        }
    }
}

/// Per-file block progress update inside a [`BepDownloadProgress`] message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepFileDownloadProgressUpdate {
    pub update_type: DownloadProgressUpdateType,
    pub name: String,
    pub version: Vec<BepVersionCounter>,
    pub block_indexes: Vec<i32>,
    pub block_size: i32,
}

/// `DownloadProgress` message (type 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepDownloadProgress {
    pub folder: String,
    pub updates: Vec<BepFileDownloadProgressUpdate>,
}

impl BepDownloadProgress {
    /// Encodes this `DownloadProgress` message into Protobuf wire format.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        if !self.folder.is_empty() {
            encode_bytes_field(&mut buf, 1, self.folder.as_bytes());
        }
        for u in &self.updates {
            let mut ubuf = Vec::new();
            if u.update_type != DownloadProgressUpdateType::Update {
                encode_varint_field(&mut ubuf, 1, u.update_type as u64);
            }
            if !u.name.is_empty() {
                encode_bytes_field(&mut ubuf, 2, u.name.as_bytes());
            }
            if !u.version.is_empty() {
                let mut vec_buf = Vec::new();
                for c in &u.version {
                    let mut cbuf = Vec::new();
                    encode_varint_field(&mut cbuf, 1, c.short_device_id);
                    encode_varint_field(&mut cbuf, 2, c.value);
                    encode_bytes_field(&mut vec_buf, 1, &cbuf);
                }
                encode_bytes_field(&mut ubuf, 3, &vec_buf);
            }
            if !u.block_indexes.is_empty() {
                // Proto3 packed repeated int32 (wire type 2)
                let mut packed = Vec::with_capacity(u.block_indexes.len() * 2);
                for &idx in &u.block_indexes {
                    encode_varint(idx as i64 as u64, &mut packed);
                }
                encode_bytes_field(&mut ubuf, 4, &packed);
            }
            if u.block_size != 0 {
                encode_varint_field(&mut ubuf, 5, u.block_size as i64 as u64);
            }
            encode_bytes_field(&mut buf, 2, &ubuf);
        }
        buf
    }

    /// Decodes a `DownloadProgress` message from Protobuf wire format.
    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut folder = String::new();
        let mut updates = Vec::new();

        for item in WireIter::new(payload) {
            let (field_num, wire_type, data) = item?;
            if wire_type == 2 {
                match field_num {
                    1 => folder = String::from_utf8_lossy(data).into_owned(),
                    2 => updates.push(Self::decode_update(data)?),
                    _ => {}
                }
            }
        }

        Ok(Self { folder, updates })
    }

    fn decode_update(bytes: &[u8]) -> io::Result<BepFileDownloadProgressUpdate> {
        let mut update_type = DownloadProgressUpdateType::Update;
        let mut name = String::new();
        let mut version = Vec::new();
        let mut block_indexes = Vec::new();
        let mut block_size = 0i32;

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            match (field_num, wire_type) {
                (1, 0) => {
                    let (v, _) = decode_varint(data)?;
                    update_type = DownloadProgressUpdateType::from_wire(v);
                }
                (2, 2) => name = String::from_utf8_lossy(data).into_owned(),
                (3, 2) => {
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
                            version.push(BepVersionCounter {
                                short_device_id: short_id,
                                value: val,
                            });
                        }
                    }
                }
                (4, 0) => {
                    // Unpacked repeated int32
                    let (v, _) = decode_varint(data)?;
                    block_indexes.push(v as i32);
                }
                (4, 2) => {
                    // Packed repeated int32
                    let mut rest = data;
                    while !rest.is_empty() {
                        let (v, used) = decode_varint(rest)?;
                        block_indexes.push(v as i32);
                        rest = &rest[used..];
                    }
                }
                (5, 0) => {
                    let (v, _) = decode_varint(data)?;
                    block_size = v as i32;
                }
                _ => {}
            }
        }

        Ok(BepFileDownloadProgressUpdate {
            update_type,
            name,
            version,
            block_indexes,
            block_size,
        })
    }
}
