//! Local filesystem storage for shared folders (`ux0:data/syncthing/sync` or custom `ux0:` paths).
//!
//! [`FsSyncStorage`] is split by responsibility:
//! - this file: folder registry (custom roots, paused state, folder types, advertised folders)
//!   and path resolution
//! - [`blocks`] — block hashing, delta comparison and block reads served to peers
//! - [`download`] — crash-safe download assembly (temp file, streaming writes, commit / abort)
//! - [`atomic`] — crash-safe file replacement (backup kept until the new file is in place)
//! - [`vita_fs`] — PS Vita filesystem quirks (`device:` roots, `mkdir`, recursive listing)
//!
//! Folder reconciliation (scans, pull planning) lives in [`crate::folder_sync`].

pub mod atomic;
pub mod blocks;
pub mod download;
pub mod vita_fs;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::bep::{BepFolder, FolderType};
use crate::device_id::DeviceId;
use vita_fs::accessible_root;

/// Local filesystem storage manager (`ux0:data/syncthing/sync` or custom paths).
#[derive(Debug, Clone)]
pub struct FsSyncStorage {
    default_root_dir: PathBuf,
    /// Custom target paths per `folder_id` (e.g. `"xkx75-ukkm3" -> "ux0:music/Test sv"`)
    custom_folder_paths: HashMap<String, PathBuf>,
    /// Folders paused by user
    paused_folders: HashSet<String>,
    /// Folder types other than the default `SendReceive` (e.g. read-only `SendOnly` shares)
    folder_types: HashMap<String, FolderType>,
    /// Devices each configured folder is shared with. A folder missing here is not shared with
    /// anyone: it is never announced, indexed, downloaded or served.
    folder_devices: HashMap<String, Vec<DeviceId>>,
    /// Locally configured folders to advertise in `ClusterConfig`
    local_folders: Vec<BepFolder>,
}

impl FsSyncStorage {
    pub fn new<P: Into<PathBuf>>(default_root_dir: P) -> Self {
        Self {
            default_root_dir: default_root_dir.into(),
            custom_folder_paths: HashMap::new(),
            paused_folders: HashSet::new(),
            folder_types: HashMap::new(),
            folder_devices: HashMap::new(),
            local_folders: Vec::new(),
        }
    }

    pub fn add_local_folder(&mut self, folder: BepFolder) {
        if let Some(existing) = self.local_folders.iter_mut().find(|f| f.id == folder.id) {
            *existing = folder;
        } else {
            self.local_folders.push(folder);
        }
    }

    pub fn local_folders(&self) -> &[BepFolder] {
        &self.local_folders
    }

    pub fn root_dir(&self) -> &Path {
        &self.default_root_dir
    }

    /// Sets a custom target directory for a given Syncthing `folder_id`.
    pub fn set_folder_path<P: Into<PathBuf>>(&mut self, folder_id: &str, target_path: P) {
        self.custom_folder_paths
            .insert(folder_id.to_owned(), accessible_root(target_path.into()));
    }

    /// Returns the effective root directory for a given `folder_id`.
    pub fn folder_root(&self, folder_id: &str) -> PathBuf {
        if let Some(custom) = self.custom_folder_paths.get(folder_id) {
            return custom.clone();
        }
        let clean_folder: String = folder_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        self.default_root_dir.join(clean_folder)
    }

    /// Enables or pauses a shared folder.
    pub fn set_folder_paused(&mut self, folder_id: &str, paused: bool) {
        if paused {
            self.paused_folders.insert(folder_id.to_owned());
        } else {
            self.paused_folders.remove(folder_id);
        }
    }

    pub fn is_folder_paused(&self, folder_id: &str) -> bool {
        self.paused_folders.contains(folder_id)
    }

    /// Sets the type of a shared folder. A `SendOnly` folder publishes its local changes and
    /// serves its blocks, but never applies a peer's changes (no download, no deletion).
    pub fn set_folder_type(&mut self, folder_id: &str, folder_type: FolderType) {
        if folder_type == FolderType::SendReceive {
            self.folder_types.remove(folder_id);
        } else {
            self.folder_types.insert(folder_id.to_owned(), folder_type);
        }
    }

    /// Type of `folder_id` (`SendReceive` unless set otherwise).
    pub fn folder_type(&self, folder_id: &str) -> FolderType {
        self.folder_types
            .get(folder_id)
            .copied()
            .unwrap_or_default()
    }

    /// Returns `true` if remote changes must never be applied to `folder_id`.
    pub fn is_folder_send_only(&self, folder_id: &str) -> bool {
        self.folder_type(folder_id) == FolderType::SendOnly
    }

    /// Declares the devices `folder_id` is shared with (replacing any previous list).
    pub fn set_folder_devices(&mut self, folder_id: &str, devices: Vec<DeviceId>) {
        self.folder_devices.insert(folder_id.to_owned(), devices);
    }

    /// Returns `true` if `folder_id` is a configured folder shared with `peer`. This is the
    /// authorization gate of every session: a folder a peer merely announces is not shared
    /// until the user linked that peer to it.
    pub fn is_folder_shared_with(&self, folder_id: &str, peer: &DeviceId) -> bool {
        self.folder_devices
            .get(folder_id)
            .is_some_and(|devices| devices.iter().any(|id| id.matches_peer(peer)))
    }

    /// Safely resolves (without `..`) the local path of a file inside a shared folder.
    pub fn resolve_safe_path(&self, folder_id: &str, rel_path: &str) -> PathBuf {
        let mut path = self.folder_root(folder_id);
        for comp in rel_path.split(['/', '\\']) {
            if !comp.is_empty() && comp != "." && comp != ".." {
                path.push(comp);
            }
        }
        path
    }

    /// Path of the local database of `folder_id`. It is keyed by the folder's current root so that
    /// pointing a share at another directory starts from a fresh database instead of advertising
    /// every previously known file as deleted.
    pub fn db_path(&self, folder_id: &str) -> PathBuf {
        let root = self.folder_root(folder_id);
        let digest = Sha256::digest(root.to_string_lossy().as_bytes());
        let clean_folder: String = folder_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        self.default_root_dir.join(format!(
            ".syncthing_db_{}_{:02x}{:02x}{:02x}{:02x}.txt",
            clean_folder, digest[0], digest[1], digest[2], digest[3]
        ))
    }
}
