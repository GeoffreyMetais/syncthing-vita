//! Send-only folders (Syncthing "Send Only"): local changes are sent to peers, while
//! remote changes are never applied locally.
//!
//! When a peer holds newer or extra files compared to the local state, the user can override
//! those remote changes ([`FsSyncStorage::override_remote_changes`]): local version vectors are
//! bumped above the peer's versions (and deletion tombstones are recorded for remote-only
//! files), so peers accept the local state as authoritative.

use std::collections::HashMap;
use std::io;

use super::ignore::IgnoreMatcher;
use crate::bep::BepFileInfo;
use crate::device_id::DeviceId;
use crate::local_db::{with_folder_db, LocalEntry};
use crate::storage::names::is_syncable_name;
use crate::storage::FsSyncStorage;
use crate::version_vector::{short_device_id, version_bump, version_ge, version_merge};

impl FsSyncStorage {
    /// Overrides remote changes of `folder_id` against the peer's index `remote` (Syncthing's
    /// "Override Changes"):
    /// - for every valid remote entry whose version is not already covered by our local entry:
    ///   - if present locally, merges the remote version into ours and bumps our counter above it;
    ///   - if absent locally (or locally deleted), records/bumps a deletion tombstone above the
    ///     remote version so peers delete the remote-only entry.
    ///
    /// Returns the number of entries overridden.
    pub fn override_remote_changes(
        &self,
        folder_id: &str,
        own_id: DeviceId,
        remote: &[BepFileInfo],
        on_progress: &mut dyn FnMut(),
    ) -> io::Result<usize> {
        let short_id = short_device_id(own_id);
        let root = self.folder_root(folder_id);
        let ignore = IgnoreMatcher::load(&root);
        let remote_by_name: HashMap<&str, &BepFileInfo> = remote
            .iter()
            .filter(|rf| !rf.invalid && !rf.versions.is_empty() && is_syncable_name(&rf.name))
            .filter(|rf| !ignore.is_ignored(&rf.name))
            .map(|rf| (rf.name.as_str(), rf))
            .collect();

        with_folder_db(&self.db_path(folder_id), folder_id, own_id, |db| {
            self.scan_into_db(folder_id, short_id, db, on_progress)?;
            let mut overridden = 0;

            let mut names: Vec<&str> = remote_by_name.keys().copied().collect();
            names.sort_unstable();
            for name in names {
                on_progress();
                let rf = remote_by_name[name];
                match db.entries.get(name).cloned() {
                    Some(local) => {
                        if version_ge(&local.version, &rf.versions) {
                            continue;
                        }
                        if local.deleted && rf.deleted {
                            // Both deleted; just adopt the merged version so they converge.
                            db.put(LocalEntry {
                                version: version_merge(&local.version, &rf.versions),
                                ..local
                            });
                            continue;
                        }
                        let merged = version_merge(&local.version, &rf.versions);
                        let bumped = version_bump(&merged, short_id);
                        db.put(LocalEntry {
                            version: bumped,
                            ..local
                        });
                        overridden += 1;
                    }
                    None => {
                        if rf.deleted {
                            continue;
                        }
                        let bumped = version_bump(&rf.versions, short_id);
                        db.put(LocalEntry {
                            name: rf.name.clone(),
                            is_dir: rf.is_dir,
                            deleted: true,
                            size: 0,
                            modified_s: rf.modified_s,
                            disk_mtime: 0,
                            sequence: 0,
                            version: bumped,
                            blocks: Vec::new(),
                        });
                        overridden += 1;
                    }
                }
            }
            Ok(overridden)
        })
    }
}
