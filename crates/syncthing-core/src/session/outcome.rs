//! Final outcome of a TLS + BEP/1.0 session with a Syncthing peer.

use std::net::SocketAddr;

use crate::bep::{BepFolder, BepHello};
use crate::device_id::DeviceId;

/// Final status of the post-`Hello` handshake session.
#[derive(Debug, Clone)]
pub enum SyncSessionStatus {
    AwaitingPairing,
    ClosedByPeer(String),
    Active {
        shared_folders: Vec<BepFolder>,
        files_up_to_date: usize,
        files_downloaded: usize,
        bytes_downloaded: u64,
        blocks_uploaded: usize,
        bytes_uploaded: u64,
    },
}

/// Complete result of a TLS + BEP/1.0 session with a Syncthing peer.
#[derive(Debug, Clone)]
pub struct PeerHandshakeResult {
    pub peer_addr: SocketAddr,
    pub peer_device_id: DeviceId,
    /// The peer's `Hello`, or `None` when it accepted the TLS connection but refused BEP (it
    /// does not know this device yet).
    pub peer_hello: Option<BepHello>,
    pub session_status: SyncSessionStatus,
}
