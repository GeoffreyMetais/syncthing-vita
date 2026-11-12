//! Local disk scans into the folder database and generation of the Vita's own `Index`.

use std::collections::HashSet;
use std::fs;
use std::io;

use super::mtime_secs;
use crate::bep::{BepFileInfo, BepIndex, FolderIndexState};
use crate::device_id::DeviceId;
use crate::local_db::{with_folder_db, FolderDb, LocalEntry};
use crate::storage::blocks::hash_file_blocks;
use crate::storage::download::recover_interrupted_commits;
use crate::storage::vita_fs::{
    collect_files_recursive, ensure_dir_exists, DiskEntryKind, DiskListing,
};
use crate::storage::FsSyncStorage;
use crate::version_vector::{short_device_id, version_bump, version_ge, version_merge};

impl FsSyncStorage {
    /// Updates the database from the files on disk: new or modified entries get a new local
    /// version, entries gone from disk become tombstones.
    pub(super) fn scan_into_db(
        &self,
        folder_id: &str,
        short_id: u64,
        db: &mut FolderDb,
    ) -> io::Result<()> {
        let root = self.folder_root(folder_id);
        ensure_dir_exists(&root)?;
        let mut listing = DiskListing::default();
        collect_files_recursive(&root, "", &mut listing)?;
        if recover_interrupted_commits(&listing.download_backups) {
            // A file was restored from its backup: list again so it is seen.
            listing = DiskListing::default();
            collect_files_recursive(&root, "", &mut listing)?;
        }
        listing.entries.sort_by(|a, b| a.rel.cmp(&b.rel));

        let mut seen = HashSet::new();
        // Entries that exist but cannot be inspected: they (and anything below them) are left
        // untouched in the database instead of being recorded as deleted.
        let mut unreadable: Vec<String> = Vec::new();
        for entry in listing.entries {
            let name = entry.rel;
            if entry.kind == DiskEntryKind::Unreadable {
                unreadable.push(name);
                continue;
            }
            let is_dir = entry.kind == DiskEntryKind::Dir;
            let Ok(meta) = fs::metadata(root.join(&name)) else {
                unreadable.push(name);
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
            .filter(|e| !is_under_any(&e.name, &unreadable))
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
        let files = db
            .sorted_entries()
            .iter()
            .map(|e| e.to_file_info())
            .collect();
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
}

/// Returns `true` if `name` is one of `parents` or lies below one of them.
fn is_under_any(name: &str, parents: &[String]) -> bool {
    parents.iter().any(|p| {
        name == p
            || name
                .strip_prefix(p.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}
