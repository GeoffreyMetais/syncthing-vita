//! Per-session folder bookkeeping: which folders are shared with the peer, the peer's known
//! files, and the local indexes we have published (pure state, no I/O).

use std::collections::HashMap;

use crate::bep::{BepFileInfo, BepFolder, BepIndex, BepMessageType};

/// Folder state of one peer session.
#[derive(Debug, Default)]
pub struct FolderSyncState {
    /// Folders shared with the peer (announced by it or configured locally for it).
    pub shared_folders: Vec<BepFolder>,
    /// Folder IDs the peer announced in its `ClusterConfig` (only those receive our `Index`).
    pub peer_folder_ids: Vec<String>,
    /// Accumulated remote file list per folder (from `Index` + `IndexUpdate`).
    pub remote_files_by_folder: HashMap<String, Vec<BepFileInfo>>,
    /// Our latest local index per folder.
    pub local_indexes: Vec<BepIndex>,
    /// `max_sequence` of the last local index built per folder.
    pub last_published_seq: HashMap<String, i64>,
    /// Folders for which a full `Index` was already sent (later ones are `IndexUpdate`s).
    index_sent_folders: Vec<String>,
}

impl FolderSyncState {
    /// Merges an incoming remote `Index` / `IndexUpdate` into the known remote files.
    pub fn merge_remote_index(&mut self, folder: &str, incoming: &[BepFileInfo]) {
        let existing = self
            .remote_files_by_folder
            .entry(folder.to_owned())
            .or_default();
        for file in incoming {
            if let Some(slot) = existing.iter_mut().find(|f| f.name == file.name) {
                *slot = file.clone();
            } else {
                existing.push(file.clone());
            }
        }
    }

    /// Known remote files of `folder` (empty if none yet).
    pub fn remote_files(&self, folder: &str) -> &[BepFileInfo] {
        self.remote_files_by_folder
            .get(folder)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Returns `true` if the peer announced `folder` in its `ClusterConfig`.
    pub fn is_peer_folder(&self, folder: &str) -> bool {
        self.peer_folder_ids.iter().any(|id| id == folder)
    }

    /// Records the latest local index of a folder.
    pub fn upsert_local_index(&mut self, index: BepIndex) {
        if let Some(slot) = self
            .local_indexes
            .iter_mut()
            .find(|li| li.folder == index.folder)
        {
            *slot = index;
        } else {
            self.local_indexes.push(index);
        }
    }

    /// Returns the latest local index of `folder`, if any.
    pub fn local_index(&self, folder: &str) -> Option<&BepIndex> {
        self.local_indexes.iter().find(|idx| idx.folder == folder)
    }

    /// Marks that a full `Index` was sent for `folder`.
    pub fn mark_index_sent(&mut self, folder: &str) {
        if !self.index_sent_folders.iter().any(|f| f == folder) {
            self.index_sent_folders.push(folder.to_owned());
        }
    }

    /// Message type for the next publication of `folder`: a full `Index` the first time,
    /// `IndexUpdate` afterwards. Marks the folder as sent.
    pub fn next_index_message_type(&mut self, folder: &str) -> BepMessageType {
        if self.index_sent_folders.iter().any(|f| f == folder) {
            BepMessageType::IndexUpdate
        } else {
            self.mark_index_sent(folder);
            BepMessageType::Index
        }
    }
}

/// Number of regular, non-deleted files in a published index (for UI events).
pub fn published_file_count(index: &BepIndex) -> usize {
    index
        .files
        .iter()
        .filter(|f| !f.is_dir && !f.deleted)
        .count()
}
