//! `Index` / `IndexUpdate` messages (types 1 and 2): file metadata, version vectors and blocks.

use std::io;

use super::BEP_BLOCK_SIZE;
use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

/// BEP `FileInfoType::FILE`.
const FILE_TYPE_FILE: u64 = 0;
/// BEP `FileInfoType::DIRECTORY`.
const FILE_TYPE_DIRECTORY: u64 = 1;
/// Deprecated BEP `FileInfoType::SYMLINK_FILE`.
const FILE_TYPE_SYMLINK_FILE: u64 = 2;
/// Deprecated BEP `FileInfoType::SYMLINK_DIRECTORY`.
const FILE_TYPE_SYMLINK_DIRECTORY: u64 = 3;
/// BEP `FileInfoType::SYMLINK`.
const FILE_TYPE_SYMLINK: u64 = 4;

/// BEP version vector counter (`Counter { uint64 id = 1; uint64 value = 2; }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BepVersionCounter {
    pub short_device_id: u64,
    pub value: u64,
}

/// Decodes a repeated `Counter` field (as in `Index` file entries, field 9, and
/// `DownloadProgress` updates, field 3): each item is a length-delimited `Counter` message
/// with `id = 1` (short device ID) and `value = 2`.
pub fn decode_version_counters(data: &[u8]) -> io::Result<Vec<BepVersionCounter>> {
    let mut counters = Vec::new();
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
            counters.push(BepVersionCounter {
                short_device_id: short_id,
                value: val,
            });
        }
    }
    Ok(counters)
}

/// Information about a file block in `Index` / `IndexUpdate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepBlockInfo {
    pub offset: i64,
    pub size: i32,
    pub hash: Vec<u8>,
    pub weak_hash: u32,
}

/// Information about a file in `Index` / `IndexUpdate`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BepFileInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub permissions: u32,
    pub modified_s: i64,
    pub modified_ns: i32,
    pub modified_by: u64,
    pub deleted: bool,
    pub invalid: bool,
    pub no_permissions: bool,
    pub sequence: i64,
    pub versions: Vec<BepVersionCounter>,
    pub blocks: Vec<BepBlockInfo>,
    pub symlink_target: Option<String>,
}

impl BepFileInfo {
    /// Returns `true` if this entry represents a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.symlink_target.is_some()
    }
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

    /// Encodes a full Protobuf `Index` or `IndexUpdate` message.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_bytes_field(&mut buf, 1, self.folder.as_bytes());

        for f in &self.files {
            let mut fbuf = Vec::new();
            encode_bytes_field(&mut fbuf, 1, f.name.as_bytes());
            if let Some(target) = &f.symlink_target {
                encode_varint_field(&mut fbuf, 2, FILE_TYPE_SYMLINK);
                if f.permissions != 0 {
                    encode_varint_field(&mut fbuf, 4, u64::from(f.permissions));
                }
                if !target.is_empty() {
                    encode_bytes_field(&mut fbuf, 17, target.as_bytes());
                }
            } else if f.is_dir {
                encode_varint_field(&mut fbuf, 2, FILE_TYPE_DIRECTORY);
                if f.permissions != 0 {
                    encode_varint_field(&mut fbuf, 4, u64::from(f.permissions));
                }
            } else {
                if f.size != 0 {
                    encode_varint_field(&mut fbuf, 3, f.size as u64);
                }
                if f.permissions != 0 {
                    encode_varint_field(&mut fbuf, 4, u64::from(f.permissions));
                }
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
            if f.no_permissions {
                encode_varint_field(&mut fbuf, 8, 1);
            }

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
            if f.modified_ns != 0 {
                encode_varint_field(&mut fbuf, 11, f.modified_ns as i64 as u64);
            }
            if f.modified_by != 0 {
                encode_varint_field(&mut fbuf, 12, f.modified_by);
            }
            if !f.is_dir && f.symlink_target.is_none() {
                // field 13: int32 block_size. Every block but the last has that size, so the
                // first one gives it (the blocks of a file pulled from a peer keep the size
                // that peer chose, 128 KiB to 16 MiB); never below the 128 KiB minimum.
                let block_size = f
                    .blocks
                    .first()
                    .map_or(0, |b| b.size.max(0) as usize)
                    .max(BEP_BLOCK_SIZE);
                encode_varint_field(&mut fbuf, 13, block_size as u64);

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
                    if b.weak_hash != 0 {
                        encode_varint_field(&mut bbuf, 4, u64::from(b.weak_hash));
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
        let mut file_type = FILE_TYPE_FILE;
        let mut size = 0i64;
        let mut permissions = 0u32;
        let mut modified_s = 0i64;
        let mut modified_ns = 0i32;
        let mut modified_by = 0u64;
        let mut deleted = false;
        let mut invalid = false;
        let mut no_permissions = false;
        let mut sequence = 0i64;
        let mut versions = Vec::new();
        let mut blocks = Vec::new();
        let mut raw_symlink_target: Option<String> = None;

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            match (field_num, wire_type) {
                (1, 2) => name = String::from_utf8_lossy(data).into_owned(),
                (2, 0) => {
                    let (v, _) = decode_varint(data)?;
                    file_type = v;
                }
                (3, 0) => {
                    let (v, _) = decode_varint(data)?;
                    size = v as i64;
                }
                (4, 0) => {
                    let (v, _) = decode_varint(data)?;
                    permissions = v as u32;
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
                (8, 0) => {
                    let (v, _) = decode_varint(data)?;
                    no_permissions = v != 0;
                }
                (9, 2) => {
                    versions.extend(decode_version_counters(data)?);
                }
                (10, 0) => {
                    let (v, _) = decode_varint(data)?;
                    sequence = v as i64;
                }
                (11, 0) => {
                    let (v, _) = decode_varint(data)?;
                    modified_ns = v as i32;
                }
                (12, 0) => {
                    let (v, _) = decode_varint(data)?;
                    modified_by = v;
                }
                (16, 2) => blocks.push(Self::decode_block_info(data)?),
                (17, 2) => raw_symlink_target = Some(String::from_utf8_lossy(data).into_owned()),
                _ => {}
            }
        }

        let is_dir = file_type == FILE_TYPE_DIRECTORY;
        let is_symlink = matches!(
            file_type,
            FILE_TYPE_SYMLINK | FILE_TYPE_SYMLINK_FILE | FILE_TYPE_SYMLINK_DIRECTORY
        );
        let unsupported_type = !is_dir && !is_symlink && file_type != FILE_TYPE_FILE;
        let symlink_target = if is_symlink {
            Some(raw_symlink_target.unwrap_or_default())
        } else {
            None
        };

        Ok(BepFileInfo {
            name,
            is_dir,
            size,
            permissions,
            modified_s,
            modified_ns,
            modified_by,
            deleted,
            invalid: invalid || unsupported_type,
            no_permissions,
            sequence,
            versions,
            blocks,
            symlink_target,
        })
    }

    fn decode_block_info(bytes: &[u8]) -> io::Result<BepBlockInfo> {
        let mut offset = 0i64;
        let mut size = 0i32;
        let mut hash = Vec::new();
        let mut weak_hash = 0u32;

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
                (4, 0) => {
                    let (v, _) = decode_varint(data)?;
                    weak_hash = v as u32;
                }
                _ => {}
            }
        }

        Ok(BepBlockInfo {
            offset,
            size,
            hash,
            weak_hash,
        })
    }
}
