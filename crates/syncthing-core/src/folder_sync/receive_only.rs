//! Receive-only folders (Syncthing "Receive Only", the app's read-only shares): the PS Vita
//! applies its peers' changes, but its own local changes are never sent to them.
//!
//! As in Syncthing, a locally changed entry is still listed in the published `Index`, flagged
//! `invalid`: peers know the device has something else there and never pull it. The user can
//! revert those local changes ([`FsSyncStorage::revert_local_changes`]) to get the peer's
//! version back.

use std::collections::HashMap;
use std::fs;
use std::io;

use crate::bep::{BepFileInfo, BepIndex, BepVersionCounter};
use crate::device_id::DeviceId;
use crate::local_db::{with_folder_db, LocalEntry};
use crate::storage::FsSyncStorage;
use crate::version_vector::{short_device_id, version_ge};

/// Valid remote entries by name.
fn valid_remote_by_name(remote: &[BepFileInfo]) -> HashMap<&str, &BepFileInfo> {
    remote
        .iter()
        .filter(|rf| !rf.invalid)
        .map(|rf| (rf.name.as_str(), rf))
        .collect()
}

/// A local entry is a local change when the peer has no valid entry of that name, when its
/// version is not covered by the peer's version (a local creation, modification or deletion
/// the peer has not seen), or when it has no version at all (reverted, waiting for the peer's
/// version to be pulled).
fn is_local_change(local: &[BepVersionCounter], remote: Option<&&BepFileInfo>) -> bool {
    local.is_empty() || remote.is_none_or(|rf| !version_ge(&rf.versions, local))
}

/// Returns `index` with every local change flagged `invalid`. Entries the peer already has, or
/// has newer, are published unchanged.
pub fn mask_local_changes(index: &BepIndex, remote: &[BepFileInfo]) -> BepIndex {
    let remote_by_name = valid_remote_by_name(remote);
    let files = index
        .files
        .iter()
        .map(|f| BepFileInfo {
            invalid: f.invalid || is_local_change(&f.versions, remote_by_name.get(f.name.as_str())),
            ..f.clone()
        })
        .collect();
    BepIndex {
        folder: index.folder.clone(),
        files,
    }
}

impl FsSyncStorage {
    /// Reverts the local changes of `folder_id` against the peer's index `remote` (Syncthing's
    /// "Revert Local Changes"):
    /// - files and directories that only exist on the PS Vita are deleted from disk;
    /// - entries modified or deleted locally lose their local version, so the next pull
    ///   (`plan_pull`) replaces them with the peer's version.
    ///
    /// Returns the number of entries reverted. Nothing is downloaded here.
    pub fn revert_local_changes(
        &self,
        folder_id: &str,
        own_id: DeviceId,
        remote: &[BepFileInfo],
        on_progress: &mut dyn FnMut(),
    ) -> io::Result<usize> {
        let short_id = short_device_id(own_id);
        let remote_by_name = valid_remote_by_name(remote);
        with_folder_db(&self.db_path(folder_id), folder_id, own_id, |db| {
            self.scan_into_db(folder_id, short_id, db, on_progress)?;
            let mut changed: Vec<LocalEntry> = db
                .entries
                .values()
                .filter(|e| is_local_change(&e.version, remote_by_name.get(e.name.as_str())))
                .cloned()
                .collect();
            // Children before their parent directory.
            changed.sort_by(|a, b| b.name.cmp(&a.name));

            let mut reverted = 0;
            for entry in changed {
                on_progress();
                if remote_by_name.contains_key(entry.name.as_str()) {
                    db.put(LocalEntry {
                        version: Vec::new(),
                        ..entry
                    });
                    reverted += 1;
                    continue;
                }
                if entry.deleted {
                    continue; // Already gone from disk; the peer never had it.
                }
                let path = self.resolve_safe_path(folder_id, &entry.name);
                let removed = if entry.is_dir {
                    fs::remove_dir(&path)
                } else {
                    fs::remove_file(&path)
                };
                if removed.is_ok() || !path.exists() {
                    reverted += 1;
                }
            }
            // Removed entries become tombstones (still unknown to the peer, so never offered).
            self.scan_into_db(folder_id, short_id, db, on_progress)?;
            Ok(reverted)
        })
    }
}
