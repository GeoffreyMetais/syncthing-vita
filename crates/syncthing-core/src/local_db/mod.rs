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
//!
//! - this file: [`LocalEntry`] and [`FolderDb`] (in-memory model and sequence bookkeeping)
//! - [`codec`] — on-disk text format (`#vita-db-v1`)
//! - [`cache`] — process-wide cache, load-on-first-use and atomic persistence
//!
//! Version vector arithmetic lives in [`crate::version_vector`].

mod cache;
mod codec;

pub use cache::with_folder_db;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::bep::{BepBlockInfo, BepFileInfo, BepVersionCounter};
use crate::device_id::DeviceId;

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
            size: if self.deleted || self.is_dir {
                0
            } else {
                self.size
            },
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
    /// Creates an empty database with a fresh random-looking, non-zero `index_id`.
    pub fn new(folder_id: &str, own_id: DeviceId) -> Self {
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
}
