//! Folder reconciliation on top of the persistent local database ([`crate::local_db`]):
//! local disk scans, pull planning against a peer's index, and local `Index` generation.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::bep::{BepFileInfo, BepIndex, FolderIndexState};
use crate::disco::DeviceId;
use crate::local_db::{
    short_device_id, version_bump, version_ge, version_merge, with_folder_db, FolderDb, LocalEntry,
};
use crate::storage::{collect_files_recursive, ensure_dir_exists, hash_file_blocks, FsSyncStorage};

/// What must happen locally to catch up with a peer's index.
#[derive(Debug, Default)]
pub struct PullPlan {
    /// Files whose newer remote version must be downloaded.
    pub downloads: Vec<BepFileInfo>,
    /// Remote files the Vita already has (same content).
    pub up_to_date: Vec<BepFileInfo>,
    /// Human-readable list of directory creations, deletions and conflict copies applied.
    pub applied: Vec<String>,
}

fn mtime_secs(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl FsSyncStorage {
    /// Updates the database from the files on disk: new or modified entries get a new local
    /// version, entries gone from disk become tombstones.
    fn scan_into_db(&self, folder_id: &str, short_id: u64, db: &mut FolderDb) -> io::Result<()> {
        let root = self.folder_root(folder_id);
        ensure_dir_exists(&root)?;
        let mut on_disk = Vec::new();
        collect_files_recursive(&root, "", &mut on_disk)?;
        on_disk.sort_by(|a, b| a.0.cmp(&b.0));

        let mut seen = HashSet::new();
        for (name, is_dir) in on_disk {
            // Skip entries that cannot be inspected instead of aborting the whole scan.
            let Ok(meta) = fs::metadata(root.join(&name)) else {
                continue;
            };
            let mtime = mtime_secs(&meta);
            let size = if is_dir { 0 } else { meta.len() as i64 };
            seen.insert(name.clone());

            let prev = db.entries.get(&name).cloned();
            if let Some(e) = &prev {
                let unchanged = !e.deleted
                    && e.is_dir == is_dir
                    && (is_dir || (e.size == size && e.disk_mtime == mtime));
                if unchanged {
                    continue;
                }
            }
            let prev_version = prev.as_ref().map(|e| e.version.clone()).unwrap_or_default();

            if is_dir {
                db.put(LocalEntry {
                    name,
                    is_dir: true,
                    deleted: false,
                    size: 0,
                    modified_s: mtime,
                    disk_mtime: mtime,
                    sequence: 0,
                    version: version_bump(&prev_version, short_id),
                    blocks: Vec::new(),
                });
                continue;
            }

            let Ok(blocks) = hash_file_blocks(&root.join(&name)) else {
                continue;
            };
            if let Some(e) = &prev {
                let same_content = !e.deleted
                    && !e.is_dir
                    && e.size == size
                    && e.blocks.len() == blocks.len()
                    && e.blocks.iter().zip(&blocks).all(|(a, b)| a.hash == b.hash);
                if same_content {
                    // Only the timestamp moved (e.g. copied back unchanged): not a new version.
                    db.touch_disk_mtime(&name, mtime);
                    continue;
                }
            }
            db.put(LocalEntry {
                name,
                is_dir: false,
                deleted: false,
                size,
                modified_s: mtime,
                disk_mtime: mtime,
                sequence: 0,
                version: version_bump(&prev_version, short_id),
                blocks,
            });
        }

        // Missing-folder guard: an empty root while the database knows live entries means the
        // folder is unavailable (unmounted card, wrong path), not that the user deleted everything.
        let live_known = db.entries.values().any(|e| !e.deleted);
        if seen.is_empty() && live_known {
            return Ok(());
        }
        let gone: Vec<LocalEntry> = db
            .entries
            .values()
            .filter(|e| !e.deleted && !seen.contains(&e.name))
            .cloned()
            .collect();
        for e in gone {
            db.put(LocalEntry {
                deleted: true,
                size: 0,
                blocks: Vec::new(),
                version: version_bump(&e.version, short_id),
                ..e
            });
        }
        Ok(())
    }

    /// Adopts remote versions for entries whose content is already identical locally, so every
    /// peer sees one single version per content instead of concurrent ones.
    fn merge_equal_remote_versions(db: &mut FolderDb, remote: &[BepFileInfo]) {
        for rf in remote {
            if rf.invalid || rf.versions.is_empty() {
                continue;
            }
            if let Some(e) = db.entries.get(&rf.name).cloned() {
                if !version_ge(&e.version, &rf.versions) && e.content_equals(rf) {
                    db.put(LocalEntry {
                        version: version_merge(&e.version, &rf.versions),
                        ..e
                    });
                }
            }
        }
    }

    fn index_from_db(folder_id: &str, db: &FolderDb) -> (BepIndex, FolderIndexState) {
        let files = db.sorted_entries().iter().map(|e| e.to_file_info()).collect();
        (
            BepIndex {
                folder: folder_id.to_owned(),
                files,
            },
            FolderIndexState {
                index_id: db.index_id,
                max_sequence: db.max_sequence,
                paused: false,
            },
        )
    }

    /// Scans the local folder and returns the Vita's `Index` for it (files, directories and
    /// deletions with persistent version vectors and sequence numbers).
    pub fn build_local_index(
        &self,
        folder_id: &str,
        own_id: DeviceId,
        remote_files: &[BepFileInfo],
    ) -> io::Result<(BepIndex, FolderIndexState)> {
        let short_id = short_device_id(own_id);
        with_folder_db(&self.db_path(folder_id), folder_id, own_id, |db| {
            self.scan_into_db(folder_id, short_id, db)?;
            Self::merge_equal_remote_versions(db, remote_files);
            Ok(Self::index_from_db(folder_id, db))
        })
    }

    /// Compares a peer's index with the local database and applies everything that needs no
    /// data transfer (directory creation, deletions, version merges, conflict copies).
    /// Returns the files that must be downloaded.
    pub fn plan_pull(
        &self,
        folder_id: &str,
        own_id: DeviceId,
        remote: &[BepFileInfo],
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
            self.scan_into_db(folder_id, short_id, db)?;
            let mut plan = PullPlan::default();
            for rf in ordered {
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
            &local.as_ref().map(|e| e.version.clone()).unwrap_or_default(),
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
                        copy.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
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
            Ok(outdated) if outdated.is_empty() && path.is_file() => {
                Self::put_file_from_remote(db, rf, &path, merged);
                plan.up_to_date.push(rf.clone());
            }
            _ => plan.downloads.push(rf.clone()),
        }
    }

    fn put_file_from_remote(
        db: &mut FolderDb,
        rf: &BepFileInfo,
        path: &std::path::Path,
        version: Vec<crate::bep::BepVersionCounter>,
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

/// `save.bin` -> `save.sync-conflict-<unix secs>-<device>.bin` (Syncthing-style conflict name).
fn conflict_copy_path(path: &std::path::Path, short_id: u64) -> std::path::PathBuf {
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
