//! Real-time events emitted during a BEP session (for the logger and the PS Vita GUI).

use std::net::SocketAddr;

use crate::bep::BepFolder;
use crate::device_id::DeviceId;

/// Real-time events emitted during a BEP session (for logger and PS Vita GUI).
#[derive(Debug, Clone)]
pub enum SyncEvent {
    PeerConnected {
        peer_addr: SocketAddr,
        peer_device_id: DeviceId,
        peer_name: String,
        client_version: String,
    },
    ClusterConfigReceived {
        peer_device_id: DeviceId,
        folders: Vec<BepFolder>,
    },
    FileUpToDate {
        folder: String,
        name: String,
        size: u64,
    },
    BlockDownloaded {
        folder: String,
        name: String,
        offset: u64,
        size: usize,
        total_size: u64,
    },
    FileSynced {
        folder: String,
        name: String,
        blocks_downloaded: usize,
        bytes_downloaded: usize,
        total_size: u64,
    },
    LocalIndexPublished {
        folder: String,
        file_count: usize,
    },
    BlockUploaded {
        folder: String,
        name: String,
        offset: u64,
        size: usize,
    },
    /// A sync cycle with `peer_device_id` completed (the first one ends the initial sync).
    SyncCycleCompleted {
        peer_device_id: DeviceId,
        active_connections: usize,
        files_up_to_date: usize,
        files_downloaded: usize,
        bytes_downloaded: usize,
        blocks_uploaded: usize,
        bytes_uploaded: usize,
    },
    /// A shared folder's local directory cannot be used; it is skipped for this session.
    FolderUnavailable {
        folder: String,
        error: String,
    },
    /// A remote change was applied without transfer (directory created, deletion, conflict copy).
    LocalChangeApplied {
        folder: String,
        description: String,
    },
    /// The local changes of a receive-only folder were reverted to the peer's version
    /// (`reverted` entries deleted or reset, then pulled again).
    LocalChangesReverted {
        folder: String,
        reverted: usize,
    },
    /// A single file could not be synchronized (e.g. local filesystem error); the session continues.
    FileError {
        folder: String,
        name: String,
        error: String,
    },
    /// The persistent session with a peer ended (with the reason, for diagnostics).
    SessionEnded {
        peer_device_id: DeviceId,
        reason: String,
    },
}
