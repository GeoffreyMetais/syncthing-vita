//! Local filesystem storage for shared folders (`ux0:data/syncthing/sync` or custom `ux0:` paths).
//!
//! [`FsSyncStorage`] is split by responsibility:
//! - this file: folder registry (custom roots, paused state, folder types, advertised folders)
//!   and path resolution
//! - [`blocks`] — block hashing, delta comparison and block reads served to peers
//! - [`download`] — crash-safe download assembly (temp file, streaming writes, commit / abort)
//! - [`names`] — Syncthing-internal names and validation of names announced by peers
//! - [`atomic`] — crash-safe file replacement (backup kept until the new file is in place)
//! - [`vita_fs`] — PS Vita filesystem quirks (`device:` roots, `mkdir`, recursive listing)
//!
//! Folder reconciliation (scans, pull planning) lives in [`crate::folder_sync`].

pub mod atomic;
pub mod blocks;
pub mod download;
pub mod encrypted;
pub mod names;
pub mod versioning;
pub mod vita_fs;
pub mod weak_hash;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::bep::{BepFolder, FolderType};
use crate::device_id::DeviceId;
use versioning::FileVersioning;
use vita_fs::accessible_root;

/// Local filesystem storage manager (`ux0:data/syncthing/sync` or custom paths).
#[derive(Debug, Clone)]
pub struct FsSyncStorage {
    default_root_dir: PathBuf,
    /// Custom target paths per `folder_id` (e.g. `"xkx75-ukkm3" -> "ux0:music/Test sv"`)
    custom_folder_paths: HashMap<String, PathBuf>,
    /// Folders paused by user
    paused_folders: HashSet<String>,
    /// Folder types other than the default `SendReceive` (e.g. read-only `ReceiveOnly` shares)
    folder_types: HashMap<String, FolderType>,
    /// File versioning policy per `folder_id` (`FileVersioning::None` when absent).
    folder_versionings: HashMap<String, FileVersioning>,
    /// Devices each configured folder is shared with. A folder missing here is not shared with
    /// anyone: it is never announced, indexed, downloaded or served.
    folder_devices: HashMap<String, Vec<DeviceId>>,
    /// Human-readable labels per configured `folder_id`.
    folder_labels: HashMap<String, String>,
    /// Locally configured folders to advertise in `ClusterConfig`
    local_folders: Vec<BepFolder>,
    /// Known local/paired devices (for introducer deduplication).
    known_devices: Vec<DeviceId>,
    /// Peers trusted as cluster introducers.
    introducer_peers: Vec<DeviceId>,
    /// Whether symbolic links are scanned and created (`false` by default, e.g. on FAT / PS Vita).
    sync_symlinks: bool,
    /// Whether POSIX permissions are ignored (`true` by default, e.g. on FAT / PS Vita).
    ignore_permissions: bool,
}

impl FsSyncStorage {
    pub fn new<P: Into<PathBuf>>(default_root_dir: P) -> Self {
        Self {
            default_root_dir: default_root_dir.into(),
            custom_folder_paths: HashMap::new(),
            paused_folders: HashSet::new(),
            folder_types: HashMap::new(),
            folder_versionings: HashMap::new(),
            folder_devices: HashMap::new(),
            folder_labels: HashMap::new(),
            local_folders: Vec::new(),
            known_devices: Vec::new(),
            introducer_peers: Vec::new(),
            sync_symlinks: false,
            ignore_permissions: true,
        }
    }

    /// Marks `device` as a known locally configured device.
    pub fn add_known_device(&mut self, device: DeviceId) {
        if !self.known_devices.iter().any(|d| d.matches_peer(&device)) {
            self.known_devices.push(device);
        }
    }

    /// Marks `device` as trusted to introduce third-party devices on shared folders.
    pub fn set_device_introducer(&mut self, device: DeviceId, introducer: bool) {
        self.add_known_device(device);
        if introducer {
            if !self
                .introducer_peers
                .iter()
                .any(|d| d.matches_peer(&device))
            {
                self.introducer_peers.push(device);
            }
        } else {
            self.introducer_peers.retain(|d| !d.matches_peer(&device));
        }
    }

    /// Returns `true` if `device` is configured as an introducer.
    pub fn is_device_introducer(&self, device: &DeviceId) -> bool {
        self.introducer_peers.iter().any(|d| d.matches_peer(device))
    }

    /// Returns `true` if `device` is already known locally (either in `known_devices` or linked
    /// to any configured folder).
    pub fn is_known_device(&self, device: &DeviceId) -> bool {
        self.known_devices.iter().any(|d| d.matches_peer(device))
            || self
                .folder_devices
                .values()
                .any(|devs| devs.iter().any(|d| d.matches_peer(device)))
    }

    /// Enables or disables symbolic link scanning and creation (OS-agnostic flag;
    /// symlinks are only materialized on Unix targets).
    pub fn set_sync_symlinks(&mut self, enabled: bool) {
        self.sync_symlinks = enabled;
    }

    /// Returns `true` if symbolic links are scanned and pulled.
    pub fn sync_symlinks(&self) -> bool {
        self.sync_symlinks
    }

    /// Configures whether POSIX permission bits are ignored (`true` for FAT/Vita; `false` on Unix
    /// to preserve `mode & 0o777`).
    pub fn set_ignore_permissions(&mut self, ignore: bool) {
        self.ignore_permissions = ignore;
    }

    /// Returns `true` if POSIX permissions are ignored (`no_permissions = true`).
    pub fn ignore_permissions(&self) -> bool {
        self.ignore_permissions
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

    /// Sets the type of a shared folder. A `ReceiveOnly` folder applies a peer's changes, but
    /// its own local changes are announced as invalid so peers never pull them
    /// (see [`crate::folder_sync::mask_local_changes`]).
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

    /// Returns `true` if the local changes of `folder_id` must never be sent to peers.
    pub fn is_folder_receive_only(&self, folder_id: &str) -> bool {
        self.folder_type(folder_id) == FolderType::ReceiveOnly
    }

    /// Returns `true` if `folder_id` is configured as [`FolderType::ReceiveEncrypted`].
    pub fn is_folder_receive_encrypted(&self, folder_id: &str) -> bool {
        self.folder_type(folder_id) == FolderType::ReceiveEncrypted
    }

    /// Returns `true` if remote changes for `folder_id` must never be applied locally.
    pub fn is_folder_send_only(&self, folder_id: &str) -> bool {
        self.folder_type(folder_id) == FolderType::SendOnly
    }

    /// Configures `.stversions` file versioning for `folder_id`.
    pub fn set_folder_versioning(&mut self, folder_id: &str, versioning: FileVersioning) {
        if versioning == FileVersioning::None {
            self.folder_versionings.remove(folder_id);
        } else {
            self.folder_versionings
                .insert(folder_id.to_owned(), versioning);
        }
    }

    /// Returns the `.stversions` file versioning policy of `folder_id`.
    pub fn folder_versioning(&self, folder_id: &str) -> FileVersioning {
        self.folder_versionings
            .get(folder_id)
            .copied()
            .unwrap_or_default()
    }

    /// Declares the devices `folder_id` is shared with (replacing any previous list).
    pub fn set_folder_devices(&mut self, folder_id: &str, devices: Vec<DeviceId>) {
        self.folder_devices.insert(folder_id.to_owned(), devices);
    }

    /// Sets the human-readable label for a configured `folder_id`.
    pub fn set_folder_label(&mut self, folder_id: &str, label: &str) {
        if label.is_empty() {
            self.folder_labels.remove(folder_id);
        } else {
            self.folder_labels
                .insert(folder_id.to_owned(), label.to_owned());
        }
    }

    /// Returns all folders shared with `peer`: explicitly added `local_folders` plus any folder
    /// registered via [`Self::set_folder_devices`] that includes `peer`, using its configured
    /// label or defaulting to the folder ID.
    pub fn configured_folders_for_peer(&self, peer: &DeviceId) -> Vec<BepFolder> {
        let mut folders: Vec<BepFolder> = self
            .local_folders
            .iter()
            .filter(|lf| lf.device_ids.iter().any(|id| id.matches_peer(peer)))
            .cloned()
            .collect();
        let mut extra: Vec<(&String, &Vec<DeviceId>)> = self
            .folder_devices
            .iter()
            .filter(|(folder_id, devices)| {
                devices.iter().any(|id| id.matches_peer(peer))
                    && !folders.iter().any(|f| &f.id == *folder_id)
            })
            .collect();
        extra.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (folder_id, devices) in extra {
            let label = self
                .folder_labels
                .get(folder_id)
                .filter(|l| !l.is_empty())
                .cloned()
                .unwrap_or_else(|| folder_id.clone());
            folders.push(BepFolder {
                id: folder_id.clone(),
                label,
                folder_type: self.folder_type(folder_id),
                device_ids: devices.clone(),
                ..Default::default()
            });
        }
        folders
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
