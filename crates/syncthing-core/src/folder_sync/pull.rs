//! Pull planning: compares a peer's index with the local database, applies everything that
//! needs no data transfer, and records completed downloads.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::ignore::IgnoreMatcher;
use super::{apply_posix_permissions, mtime_secs, PullPlan};
use crate::bep::{BepFileInfo, BepVersionCounter};
use crate::device_id::DeviceId;
use crate::local_db::{
    flush_folder_db, with_folder_db, with_folder_db_deferred, FolderDb, LocalEntry,
};
use crate::storage::names::is_syncable_name;
use crate::storage::vita_fs::ensure_dir_exists;
use crate::storage::FsSyncStorage;
use crate::version_vector::{short_device_id, version_ge, version_merge};

impl FsSyncStorage {
    /// Compares a peer's index with the local database and applies everything that needs no
    /// data transfer (directory creation, deletions, symlinks, version merges, conflict copies).
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
        let root = self.folder_root(folder_id);
        let ignore = IgnoreMatcher::load(&root);
        let send_only = self.is_folder_send_only(folder_id);
        // Names that do not designate a plain entry inside the folder (or are ignored by
        // `.stignore`) are skipped. Parents before children for creations; children before parents
        // for deletions.
        let mut ordered: Vec<&BepFileInfo> = remote
            .iter()
            .filter(|rf| {
                !rf.invalid
                    && !rf.versions.is_empty()
                    && is_syncable_name(&rf.name)
                    && !ignore.is_ignored(&rf.name)
            })
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
            if send_only {
                return Ok(plan);
            }
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
        let is_symlink = rf.symlink_target.is_some();
        if is_symlink && !rf.deleted && !self.sync_symlinks() {
            // Symlinks are unsupported on platforms such as PS Vita / FAT: skip safely without error.
            return;
        }
        let is_regular = !rf.deleted && !rf.is_dir && !is_symlink;
        // The common case (nothing newer on the peer) is decided on a borrowed entry.
        if let Some(e) = db.entries.get(&rf.name) {
            if version_ge(&e.version, &rf.versions) {
                if is_regular && e.content_equals(rf) {
                    plan.up_to_date.push(rf.clone());
                }
                return;
            }
        }
        let local = db.entries.get(&rf.name).cloned();

        let path = self.resolve_safe_path(folder_id, &rf.name);

        if let Some(e) = &local {
            if e.content_equals(rf) {
                if !rf.deleted && !self.ignore_permissions() && !rf.no_permissions && !is_symlink {
                    apply_posix_permissions(&path, rf.permissions);
                }
                db.put(LocalEntry {
                    permissions: if self.ignore_permissions() {
                        e.permissions
                    } else if !rf.no_permissions && rf.permissions != 0 {
                        rf.permissions
                    } else {
                        e.permissions
                    },
                    modified_ns: rf.modified_ns,
                    modified_by: rf.modified_by,
                    version: version_merge(&e.version, &rf.versions),
                    ..e.clone()
                });
                if is_regular {
                    plan.up_to_date.push(rf.clone());
                }
                return;
            }
        }

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
            if !e.deleted && !e.is_dir && e.symlink_target.is_none() && path.is_file() {
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
                let is_link = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
                let removed = if !is_link && path.is_dir() {
                    fs::remove_dir(&path)
                } else {
                    fs::remove_file(&path)
                };
                if removed.is_err() && fs::symlink_metadata(&path).is_ok() {
                    return; // Non-empty directory or locked file: retry on the next index.
                }
                plan.applied.push(format!("Deleted {}", rf.name));
            }
            db.put(LocalEntry {
                deleted: true,
                size: 0,
                modified_by: rf.modified_by,
                modified_ns: rf.modified_ns,
                blocks: Vec::new(),
                symlink_target: None,
                version: merged,
                ..e
            });
            return;
        }

        if let Some(target) = &rf.symlink_target {
            if target.is_empty() || !self.is_link_path_inside_root(folder_id, &path) {
                return;
            }
            if let Some(parent) = path.parent() {
                if ensure_dir_exists(parent).is_err() {
                    return;
                }
            }
            if create_or_replace_symlink(target, &path).is_err() {
                return;
            }
            let disk_mtime = fs::symlink_metadata(&path)
                .map(|m| mtime_secs(&m))
                .unwrap_or(0);
            plan.applied
                .push(format!("Created symlink {} -> {}", rf.name, target));
            db.put(LocalEntry {
                name: rf.name.clone(),
                is_dir: false,
                deleted: false,
                size: 0,
                permissions: rf.permissions,
                no_permissions: self.ignore_permissions(),
                modified_s: rf.modified_s,
                modified_ns: rf.modified_ns,
                modified_by: rf.modified_by,
                disk_mtime,
                sequence: 0,
                version: merged,
                blocks: Vec::new(),
                symlink_target: Some(target.clone()),
            });
            return;
        }

        if rf.is_dir {
            if ensure_dir_exists(&path).is_err() {
                return;
            }
            if !self.ignore_permissions() && !rf.no_permissions {
                apply_posix_permissions(&path, rf.permissions);
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
                permissions: if self.ignore_permissions() {
                    0o755
                } else {
                    rf.permissions
                },
                no_permissions: self.ignore_permissions(),
                modified_s: rf.modified_s,
                modified_ns: rf.modified_ns,
                modified_by: rf.modified_by,
                disk_mtime: mtime,
                sequence: 0,
                version: merged,
                blocks: Vec::new(),
                symlink_target: None,
            });
            return;
        }

        // Regular file: the content may still match when the peer uses another block size.
        match self.find_outdated_blocks(folder_id, rf) {
            // A local file with the right blocks but another length (trailing data) goes
            // through a download too: it is fixed in the temp file, never in place.
            Ok(outdated) if outdated.is_empty() && has_len(&path, rf.size) => {
                if !self.ignore_permissions() && !rf.no_permissions {
                    apply_posix_permissions(&path, rf.permissions);
                }
                self.put_file_from_remote(db, rf, &path, merged);
                plan.up_to_date.push(rf.clone());
            }
            _ => plan.downloads.push(rf.clone()),
        }
    }

    /// Validates that `link_path` itself (not its target) resides strictly inside the folder root.
    fn is_link_path_inside_root(&self, folder_id: &str, link_path: &Path) -> bool {
        let root = self.folder_root(folder_id);
        let Ok(rel) = link_path.strip_prefix(&root) else {
            return false;
        };
        !rel.as_os_str().is_empty()
            && rel
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
    }

    fn put_file_from_remote(
        &self,
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
            permissions: if self.ignore_permissions() {
                0o644
            } else {
                rf.permissions
            },
            no_permissions: self.ignore_permissions(),
            modified_s: rf.modified_s,
            modified_ns: rf.modified_ns,
            modified_by: rf.modified_by,
            disk_mtime,
            sequence: 0,
            version,
            blocks: rf.blocks.clone(),
            symlink_target: None,
        });
    }

    /// Records a completed download of `rf` with the remote's version. The database is not
    /// rewritten for every file: call [`Self::flush_folder_db`] once the downloads are over.
    pub fn record_downloaded(&self, folder_id: &str, own_id: DeviceId, rf: &BepFileInfo) {
        let path = self.resolve_safe_path(folder_id, &rf.name);
        if !self.ignore_permissions() && !rf.no_permissions {
            apply_posix_permissions(&path, rf.permissions);
        }
        with_folder_db_deferred(&self.db_path(folder_id), folder_id, own_id, |db| {
            let prev = db
                .entries
                .get(&rf.name)
                .map(|e| e.version.clone())
                .unwrap_or_default();
            self.put_file_from_remote(db, rf, &path, version_merge(&prev, &rf.versions));
        });
    }

    /// Saves the pending database changes of `folder_id` (see [`Self::record_downloaded`]).
    pub fn flush_folder_db(&self, folder_id: &str) {
        flush_folder_db(&self.db_path(folder_id));
    }
}

#[cfg(unix)]
fn create_or_replace_symlink(target: &str, dest: &Path) -> io::Result<()> {
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dest.with_file_name(format!(".syncthing.{file_name}.symlink.tmp"));
    let _ = fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp)?;
    if let Ok(meta) = fs::symlink_metadata(dest) {
        if meta.file_type().is_dir() {
            let _ = fs::remove_dir_all(dest);
        }
    }
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_or_replace_symlink(_target: &str, _dest: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symbolic links are not supported on this platform",
    ))
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
