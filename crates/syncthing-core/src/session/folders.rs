//! Per-session folder bookkeeping: which folders are shared with the peer, the peer's known
//! files, and what we already published of our local indexes (pure state, no I/O).

use std::collections::HashMap;

use crate::bep::{BepFileInfo, BepFolder, BepIndex, BepMessageType};

/// Maximum number of files per `Index` / `IndexUpdate` message (as Syncthing does), so a large
/// folder is never encoded as one huge message.
pub const INDEX_BATCH_FILES: usize = 1000;

/// The peer's known files of one folder (from `Index` + `IndexUpdate`), with a name lookup.
#[derive(Debug, Default)]
pub struct RemoteFiles {
    files: Vec<BepFileInfo>,
    positions: HashMap<String, usize>,
}

impl RemoteFiles {
    /// Inserts or replaces the entries of `incoming`.
    fn merge(&mut self, incoming: &[BepFileInfo]) {
        for file in incoming {
            match self.positions.get(&file.name) {
                Some(&pos) => self.files[pos] = file.clone(),
                None => {
                    self.positions.insert(file.name.clone(), self.files.len());
                    self.files.push(file.clone());
                }
            }
        }
    }

    pub fn files(&self) -> &[BepFileInfo] {
        &self.files
    }
}

/// Folder state of one peer session.
#[derive(Debug, Default)]
pub struct FolderSyncState {
    /// Folders shared with the peer: only folders the local configuration links to this peer
    /// (announced by it or offered by us), never a folder the peer merely announces.
    pub shared_folders: Vec<BepFolder>,
    /// Shared folder IDs the peer announced in its `ClusterConfig` (only those receive our `Index`).
    pub peer_folder_ids: Vec<String>,
    /// Accumulated remote file list per folder.
    remote_files_by_folder: HashMap<String, RemoteFiles>,
    /// Our latest local index per folder.
    pub local_indexes: Vec<BepIndex>,
    /// Highest local sequence already sent to the peer per folder. A folder is present once
    /// its full `Index` was sent; later publications are `IndexUpdate`s of newer entries only.
    published_seq: HashMap<String, i64>,
}

impl FolderSyncState {
    /// Merges an incoming remote `Index` / `IndexUpdate` into the known remote files.
    pub fn merge_remote_index(&mut self, folder: &str, incoming: &[BepFileInfo]) {
        self.remote_files_by_folder
            .entry(folder.to_owned())
            .or_default()
            .merge(incoming);
    }

    /// Known remote files of `folder` (empty if none yet).
    pub fn remote_files(&self, folder: &str) -> &[BepFileInfo] {
        self.remote_files_by_folder
            .get(folder)
            .map(|r| r.files())
            .unwrap_or(&[])
    }

    /// Every folder the peer sent files for, with its known remote files.
    pub fn remote_folders(&self) -> impl Iterator<Item = (&str, &[BepFileInfo])> {
        self.remote_files_by_folder
            .iter()
            .map(|(id, remote)| (id.as_str(), remote.files()))
    }

    /// Returns `true` if `folder` is shared with the peer in this session (authorized by the
    /// local configuration). Messages about any other folder are ignored.
    pub fn is_shared_folder(&self, folder: &str) -> bool {
        self.shared_folders.iter().any(|f| f.id == folder)
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

    /// Messages to send so the peer knows `index`, and marks them as published.
    ///
    /// The first publication of a folder is a full `Index`; later ones are `IndexUpdate`s
    /// holding only the entries whose sequence is newer than what was already sent (none if
    /// nothing changed). Entries are split into batches of [`INDEX_BATCH_FILES`].
    pub fn take_unpublished(&mut self, index: &BepIndex) -> Vec<(BepMessageType, BepIndex)> {
        let already_sent = self.published_seq.get(&index.folder).copied();
        let pending: Vec<&BepFileInfo> = index
            .files
            .iter()
            .filter(|f| already_sent.is_none_or(|seq| f.sequence > seq))
            .collect();
        let newest = index
            .files
            .iter()
            .map(|f| f.sequence)
            .max()
            .unwrap_or(0)
            .max(already_sent.unwrap_or(0));
        self.published_seq.insert(index.folder.clone(), newest);

        let batch = |files: &[&BepFileInfo]| BepIndex {
            folder: index.folder.clone(),
            files: files.iter().map(|f| (*f).clone()).collect(),
        };
        let mut messages = Vec::new();
        if already_sent.is_none() && pending.is_empty() {
            // An empty folder still announces itself with an (empty) full `Index`.
            messages.push((BepMessageType::Index, batch(&[])));
        }
        for chunk in pending.chunks(INDEX_BATCH_FILES) {
            let msg_type = if already_sent.is_none() && messages.is_empty() {
                BepMessageType::Index
            } else {
                BepMessageType::IndexUpdate
            };
            messages.push((msg_type, batch(chunk)));
        }
        messages
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
