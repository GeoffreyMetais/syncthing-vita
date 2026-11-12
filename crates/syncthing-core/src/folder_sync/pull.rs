//! Pull planning: compares a peer's index with the local database, applies everything that
//! needs no data transfer, and records completed downloads.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{mtime_secs, PullPlan};
use crate::bep::{BepFileInfo, BepVersionCounter};
use crate::device_id::DeviceId;
use crate::local_db::{with_folder_db, FolderDb, LocalEntry};
use crate::storage::vita_fs::ensure_dir_exists;
use crate::storage::FsSyncStorage;
use crate::version_vector::{short_device_id, version_ge, version_merge};

impl FsSyncStorage {
    /// Compares a peer's index with the local database and applies everything that needs no
    /// data transfer (directory creation, deletions, version merges, conflict copies).
    /// Returns the files that must be downloaded.
    pub fn plan_pull(
        &self,
        folder_id: &str,
        own_id: DeviceId,
        remote: &[BepFileInfo],
    ) -> io::Result<PullPlan> {
        self.plan_pull_with_progress(folder_id, own_id, remote, &mut || {})
    }

    /// Same as [`Self::plan_pull`]; `on_progress` is called regularly while scanning and
    /// planning so a session can keep its connection alive.
    pub fn plan_pull_with_progress(
        &self,
        folder_id: &str,
        own_id: DeviceId,
        remote: &[BepFileInfo],
        on_progress: &mut dyn FnMut(),
    ) -> io::Result<PullPlan> {
        let short_id = short_device_id(own_id);
        // Parents before children for creations; children before parents for deletions.
        let mut ordered: Vec<&BepFileInfo> = remote
            .iter()
            .filter(|rf| !rf.invalid && !rf.versions.is_empty())
            .collect();
        ordered.sort_by(|a, b| match (a.deleted, b.deleted) {
            (false, false) => a.name.cmp(&b.name),
            (true, true) => b.name.cmp(&a.name),
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
        });

        with_folder_db(&self.db_path(folder_id), folder_id, own_id, |db| {
            self.scan_into_db(folder_id, short_id, db, on_progress)?;
            let mut plan = PullPlan::default();
            for rf in ordered {
                on_progress();
                self.plan_entry(folder_id, short_id, db, rf, &mut plan);
            }
            Ok(plan)
        })
    }

    fn plan_entry(
        &self,
        folder_id: &str,
        short_id: u64,
        db: &mut FolderDb,
        rf: &BepFileInfo,
        plan: &mut PullPlan,
    ) {
        let local = db.entries.get(&rf.name).cloned();
        let is_regular = !rf.deleted && !rf.is_dir;

        if let Some(e) = &local {
            if version_ge(&e.version, &rf.versions) {
                if is_regular && e.content_equals(rf) {
                    plan.up_to_date.push(rf.clone());
                }
                return;
            }
            if e.content_equals(rf) {
                db.put(LocalEntry {
                    version: version_merge(&e.version, &rf.versions),
                    ..e.clone()
                });
                if is_regular {
                    plan.up_to_date.push(rf.clone());
                }
                return;
            }
        }

        let path = self.resolve_safe_path(folder_id, &rf.name);
        let merged = version_merge(
            &local
                .as_ref()
                .map(|e| e.version.clone())
                .unwrap_or_default(),
            &rf.versions,
        );
        let concurrent = local
            .as_ref()
            .is_some_and(|e| !version_ge(&rf.versions, &e.version));
        if concurrent {
            // Conflict, resolved like Syncthing: a deletion always loses, then the newest wins.
            let e = local.as_ref().expect("concurrent implies a local entry");
            let remote_wins = match (e.deleted, rf.deleted) {
                (true, false) => true,
                (false, true) => false,
                _ => rf.modified_s > e.modified_s,
            };
            if !remote_wins {
                // The peer resolves the same conflict in our favour and pulls our version.
                return;
            }
            if !e.deleted && !e.is_dir && path.is_file() {
                let copy = conflict_copy_path(&path, short_id);
                if fs::rename(&path, &copy).is_ok() {
                    plan.applied.push(format!(
                        "Conflict on {}: kept local copy as {}",
                        rf.name,
                        copy.file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default()
                    ));
                }
            }
        }

        if rf.deleted {
            let Some(e) = local else {
                return; // Never had it: nothing to delete.
            };
            if !e.deleted {
                let removed = if path.is_dir() {
                    fs::remove_dir(&path)
                } else {
                    fs::remove_file(&path)
                };
                if removed.is_err() && path.exists() {
                    return; // Non-empty directory or locked file: retry on the next index.
                }
                plan.applied.push(format!("Deleted {}", rf.name));
            }
            db.put(LocalEntry {
                deleted: true,
                size: 0,
                blocks: Vec::new(),
                version: merged,
                ..e
            });
            return;
        }

        if rf.is_dir {
            if ensure_dir_exists(&path).is_err() {
                return;
            }
            let mtime = fs::metadata(&path).map(|m| mtime_secs(&m)).unwrap_or(0);
            if local.as_ref().is_none_or(|e| e.deleted || !e.is_dir) {
                plan.applied.push(format!("Created directory {}", rf.name));
            }
            db.put(LocalEntry {
                name: rf.name.clone(),
                is_dir: true,
                deleted: false,
                size: 0,
                modified_s: rf.modified_s,
                disk_mtime: mtime,
                sequence: 0,
                version: merged,
                blocks: Vec::new(),
            });
            return;
        }

        // Regular file: the content may still match when the peer uses another block size.
        match self.find_outdated_blocks(folder_id, rf) {
            // A local file with the right blocks but another length (trailing data) goes
            // through a download too: it is fixed in the temp file, never in place.
            Ok(outdated) if outdated.is_empty() && has_len(&path, rf.size) => {
                Self::put_file_from_remote(db, rf, &path, merged);
                plan.up_to_date.push(rf.clone());
            }
            _ => plan.downloads.push(rf.clone()),
        }
    }

    fn put_file_from_remote(
        db: &mut FolderDb,
        rf: &BepFileInfo,
        path: &Path,
        version: Vec<BepVersionCounter>,
    ) {
        let disk_mtime = fs::metadata(path).map(|m| mtime_secs(&m)).unwrap_or(0);
        db.put(LocalEntry {
            name: rf.name.clone(),
            is_dir: false,
            deleted: false,
            size: rf.size,
            modified_s: rf.modified_s,
            disk_mtime,
            sequence: 0,
            version,
            blocks: rf.blocks.clone(),
        });
    }

    /// Records a completed download of `rf` with the remote's version.
    pub fn record_downloaded(&self, folder_id: &str, own_id: DeviceId, rf: &BepFileInfo) {
        let path = self.resolve_safe_path(folder_id, &rf.name);
        with_folder_db(&self.db_path(folder_id), folder_id, own_id, |db| {
            let prev = db
                .entries
                .get(&rf.name)
                .map(|e| e.version.clone())
                .unwrap_or_default();
            Self::put_file_from_remote(db, rf, &path, version_merge(&prev, &rf.versions));
        });
    }
}

/// Returns `true` if `path` is a regular file of exactly `size` bytes.
fn has_len(path: &Path, size: i64) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == size.max(0) as u64)
}

/// `save.bin` -> `save.sync-conflict-<unix secs>-<device>.bin` (Syncthing-style conflict name).
fn conflict_copy_path(path: &Path, short_id: u64) -> PathBuf {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tag = format!("sync-conflict-{}-{:07X}", secs, short_id >> 36);
    let name = match path.extension() {
        Some(ext) => format!("{}.{}.{}", stem, tag, ext.to_string_lossy()),
        None => format!("{}.{}", stem, tag),
    };
    path.with_file_name(name)
}
