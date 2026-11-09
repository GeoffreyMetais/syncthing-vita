//! Persistent per-folder local file database (the Vita's own view of each shared folder).
//!
//! Syncthing peers compare **version vectors**, not file contents, to decide who is up to date.
//! The Vita therefore keeps one authoritative entry per file / directory / deletion with:
//! - a version vector that is only bumped by real local changes and merged with remote versions
//!   when the content is identical, so every peer sees the same version for the same content;
//! - a monotonically increasing sequence number and a stable index ID per folder;
//! - tombstones for deleted entries.
//!
//! The database is shared by all concurrent peer sessions through a process-wide cache.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::bep::{BepBlockInfo, BepFileInfo, BepVersionCounter};
use crate::disco::DeviceId;

/// One local entry (file, directory or tombstone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    pub name: String,
    pub is_dir: bool,
    pub deleted: bool,
    pub size: i64,
    /// Modification time advertised to peers.
    pub modified_s: i64,
    /// Last observed on-disk modification time (change detection only).
    pub disk_mtime: i64,
    pub sequence: i64,
    pub version: Vec<BepVersionCounter>,
    pub blocks: Vec<BepBlockInfo>,
}

impl LocalEntry {
    pub fn to_file_info(&self) -> BepFileInfo {
        BepFileInfo {
            name: self.name.clone(),
            is_dir: self.is_dir,
            size: if self.deleted || self.is_dir { 0 } else { self.size },
            modified_s: self.modified_s,
            deleted: self.deleted,
            invalid: false,
            sequence: self.sequence,
            versions: self.version.clone(),
            blocks: if self.deleted || self.is_dir {
                Vec::new()
            } else {
                self.blocks.clone()
            },
        }
    }

    /// Returns `true` if this entry has the same content as `rf` (type, deletion state, data).
    pub fn content_equals(&self, rf: &BepFileInfo) -> bool {
        if self.deleted || rf.deleted {
            return self.deleted && rf.deleted;
        }
        if self.is_dir || rf.is_dir {
            return self.is_dir && rf.is_dir;
        }
        self.size == rf.size
            && self.blocks.len() == rf.blocks.len()
            && self
                .blocks
                .iter()
                .zip(rf.blocks.iter())
                .all(|(a, b)| a.size == b.size && a.hash == b.hash)
    }
}

/// The local database of one folder.
#[derive(Debug, Clone, Default)]
pub struct FolderDb {
    pub index_id: u64,
    pub max_sequence: i64,
    pub entries: HashMap<String, LocalEntry>,
    dirty: bool,
}

impl FolderDb {
    fn new(folder_id: &str, own_id: DeviceId) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut h = Sha256::new();
        h.update(b"vita-bep-index-id:");
        h.update(folder_id.as_bytes());
        h.update(own_id.0);
        h.update(nanos.to_le_bytes());
        let d = h.finalize();
        let index_id = u64::from_be_bytes([d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]]) | 1;
        Self {
            index_id,
            max_sequence: 0,
            entries: HashMap::new(),
            dirty: true,
        }
    }

    /// Stores `entry` with a fresh sequence number.
    pub fn put(&mut self, mut entry: LocalEntry) {
        self.max_sequence += 1;
        entry.sequence = self.max_sequence;
        self.entries.insert(entry.name.clone(), entry);
        self.dirty = true;
    }

    /// Updates only on-disk bookkeeping (no new sequence, nothing advertised changes).
    pub fn touch_disk_mtime(&mut self, name: &str, disk_mtime: i64) {
        if let Some(e) = self.entries.get_mut(name) {
            if e.disk_mtime != disk_mtime {
                e.disk_mtime = disk_mtime;
                self.dirty = true;
            }
        }
    }

    /// All entries ordered by sequence number.
    pub fn sorted_entries(&self) -> Vec<&LocalEntry> {
        let mut v: Vec<&LocalEntry> = self.entries.values().collect();
        v.sort_by_key(|e| e.sequence);
        v
    }

    fn serialize(&self) -> String {
        let mut out = format!("#vita-db-v1\t{:x}\t{}\n", self.index_id, self.max_sequence);
        for e in self.sorted_entries() {
            let ver: Vec<String> = e
                .version
                .iter()
                .map(|c| format!("{:x}:{}", c.short_device_id, c.value))
                .collect();
            let blocks: Vec<String> = e
                .blocks
                .iter()
                .map(|b| format!("{}:{}:{}", b.offset, b.size, to_hex(&b.hash)))
                .collect();
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                if e.is_dir { "D" } else { "F" },
                u8::from(e.deleted),
                e.size,
                e.modified_s,
                e.disk_mtime,
                e.sequence,
                ver.join(","),
                blocks.join(","),
                e.name
            ));
        }
        out
    }

    fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next()?.split('\t').collect();
        if header.len() != 3 || header[0] != "#vita-db-v1" {
            return None;
        }
        let mut db = Self {
            index_id: u64::from_str_radix(header[1], 16).ok()?,
            max_sequence: header[2].parse().ok()?,
            entries: HashMap::new(),
            dirty: false,
        };
        for line in lines {
            let p: Vec<&str> = line.splitn(9, '\t').collect();
            if p.len() != 9 || p[8].is_empty() {
                continue;
            }
            let version = p[6]
                .split(',')
                .filter(|s| !s.is_empty())
                .filter_map(|c| {
                    let (id, val) = c.split_once(':')?;
                    Some(BepVersionCounter {
                        short_device_id: u64::from_str_radix(id, 16).ok()?,
                        value: val.parse().ok()?,
                    })
                })
                .collect();
            let blocks = p[7]
                .split(',')
                .filter(|s| !s.is_empty())
                .filter_map(|b| {
                    let mut it = b.splitn(3, ':');
                    Some(BepBlockInfo {
                        offset: it.next()?.parse().ok()?,
                        size: it.next()?.parse().ok()?,
                        hash: from_hex(it.next()?)?,
                    })
                })
                .collect();
            let entry = LocalEntry {
                name: p[8].to_owned(),
                is_dir: p[0] == "D",
                deleted: p[1] == "1",
                size: p[2].parse().unwrap_or(0),
                modified_s: p[3].parse().unwrap_or(0),
                disk_mtime: p[4].parse().unwrap_or(0),
                sequence: p[5].parse().unwrap_or(0),
                version,
                blocks,
            };
            db.max_sequence = db.max_sequence.max(entry.sequence);
            db.entries.insert(entry.name.clone(), entry);
        }
        Some(db)
    }
}

fn db_cache() -> &'static Mutex<HashMap<PathBuf, FolderDb>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, FolderDb>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Runs `f` on the folder database stored at `db_path` while holding the process-wide lock,
/// loading it on first use and persisting it afterwards if it changed.
pub fn with_folder_db<R>(
    db_path: &Path,
    folder_id: &str,
    own_id: DeviceId,
    f: impl FnOnce(&mut FolderDb) -> R,
) -> R {
    let mut cache = db_cache().lock().unwrap_or_else(|p| p.into_inner());
    let db = cache.entry(db_path.to_path_buf()).or_insert_with(|| {
        fs::read_to_string(db_path)
            .ok()
            .and_then(|t| FolderDb::parse(&t))
            .unwrap_or_else(|| FolderDb::new(folder_id, own_id))
    });
    let result = f(db);
    if db.dirty && save_db(db_path, db).is_ok() {
        db.dirty = false;
    }
    result
}

fn save_db(db_path: &Path, db: &FolderDb) -> io::Result<()> {
    if let Some(parent) = db_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = db_path.with_extension("tmp");
    fs::write(&tmp, db.serialize())?;
    let _ = fs::remove_file(db_path);
    fs::rename(&tmp, db_path)
}

/// The 64-bit short device ID used in version vectors.
pub fn short_device_id(id: DeviceId) -> u64 {
    u64::from_be_bytes([
        id.0[0], id.0[1], id.0[2], id.0[3], id.0[4], id.0[5], id.0[6], id.0[7],
    ])
}

/// `a >= b`: `a` is equal to or newer than `b` for every device counter.
pub fn version_ge(a: &[BepVersionCounter], b: &[BepVersionCounter]) -> bool {
    b.iter().all(|cb| {
        cb.value == 0
            || a
                .iter()
                .any(|ca| ca.short_device_id == cb.short_device_id && ca.value >= cb.value)
    })
}

/// Element-wise maximum of two version vectors.
pub fn version_merge(a: &[BepVersionCounter], b: &[BepVersionCounter]) -> Vec<BepVersionCounter> {
    let mut out = a.to_vec();
    for cb in b {
        if let Some(ca) = out.iter_mut().find(|c| c.short_device_id == cb.short_device_id) {
            ca.value = ca.value.max(cb.value);
        } else {
            out.push(*cb);
        }
    }
    out.sort_by_key(|c| c.short_device_id);
    out
}

/// Records a local change by `short_id` (same rule as Syncthing: `max(current + 1, now)`).
pub fn version_bump(v: &[BepVersionCounter], short_id: u64) -> Vec<BepVersionCounter> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1);
    let mut out = v.to_vec();
    if let Some(c) = out.iter_mut().find(|c| c.short_device_id == short_id) {
        c.value = (c.value + 1).max(now);
    } else {
        out.push(BepVersionCounter {
            short_device_id: short_id,
            value: now,
        });
    }
    out.sort_by_key(|c| c.short_device_id);
    out
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: u64, v: u64) -> BepVersionCounter {
        BepVersionCounter {
            short_device_id: id,
            value: v,
        }
    }

    #[test]
    fn version_vector_ordering() {
        assert!(version_ge(&[c(1, 5)], &[c(1, 5)]));
        assert!(version_ge(&[c(1, 5), c(2, 1)], &[c(1, 5)]));
        assert!(!version_ge(&[c(1, 5)], &[c(2, 1)]));
        assert_eq!(version_merge(&[c(1, 5)], &[c(2, 3), c(1, 2)]), vec![c(1, 5), c(2, 3)]);
        assert!(version_bump(&[c(1, 5)], 1)[0].value > 5);
    }

    #[test]
    fn db_serialization_roundtrip() {
        let mut db = FolderDb::new("f", DeviceId([7; 32]));
        db.put(LocalEntry {
            name: "dir a/save|1.bin".to_owned(),
            is_dir: false,
            deleted: false,
            size: 3,
            modified_s: 10,
            disk_mtime: 11,
            sequence: 0,
            version: vec![c(0xABCD, 42)],
            blocks: vec![BepBlockInfo {
                offset: 0,
                size: 3,
                hash: vec![1, 2, 255],
            }],
        });
        let parsed = FolderDb::parse(&db.serialize()).expect("parse");
        assert_eq!(parsed.index_id, db.index_id);
        assert_eq!(parsed.max_sequence, 1);
        assert_eq!(parsed.entries, db.entries);
    }
}
