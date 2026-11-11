//! `Index` / `IndexUpdate` messages (types 1 and 2): file metadata, version vectors and blocks.

use std::io;

use super::BEP_BLOCK_SIZE;
use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

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
