//! One single-cycle session with one peer: its endpoints are tried in turn until one session
//! is established, which then runs the cycle (pull, publish, drain, `Close`).

use std::sync::Arc;

use syncthing_core::{
    connect_and_sync, connect_and_sync_stream, connect_via_relay, BepFolder, BepHello, DeviceId,
    DeviceIdentity, FsSyncStorage, PeerEndpoint, SessionParams, SyncControl, SyncEvent,
    SyncSessionStatus,
};

use crate::config::DeviceRecord;
use crate::defaults::{CONNECT_TIMEOUT, GLOBAL_DISCOVERY_TIMEOUT};
use crate::endpoints::{endpoint_label, KnownEndpoints};
use crate::log::WorkerLog;

/// What the cycle got from one peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerOutcome {
    /// A session ran its cycle (or was stopped while running it).
    Synced(PeerStats),
    /// The peer has not accepted this device yet.
    AwaitingPairing,
    /// The peer closed the session before its cycle was over.
    ClosedByPeer(String),
    /// No endpoint, or none answered.
    Unreachable,
    /// A stop was requested before a session was established.
    Stopped,
}

/// Transfer counters of a peer's session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PeerStats {
    pub files_downloaded: usize,
    pub bytes_downloaded: u64,
    pub blocks_uploaded: usize,
    pub bytes_uploaded: u64,
}

/// Receives the folders a peer lists in its `ClusterConfig`, with its verified device ID.
pub type OfferSink<'a> = dyn FnMut(DeviceId, Vec<BepFolder>) + 'a;

/// What a peer session needs, shared by every peer of a cycle.
pub struct SessionShared<'a> {
    pub identity: &'a DeviceIdentity,
    pub hello: &'a BepHello,
    pub storage: &'a FsSyncStorage,
    pub control: &'a Arc<SyncControl>,
    pub log: &'a WorkerLog,
}

/// Synchronizes with `peer` through the first endpoint that works. `on_offer` receives the
/// peer's verified device ID and the folders of every `ClusterConfig` it sends.
pub fn sync_with_peer(
    shared: &SessionShared<'_>,
    peer: &DeviceRecord,
    endpoints: &KnownEndpoints,
    on_offer: &mut OfferSink<'_>,
) -> PeerOutcome {
    let (target_id, targets) = endpoints.targets(peer);
    let mut outcome = PeerOutcome::Unreachable;
    for ep in targets {
        if shared.control.is_stop_requested() {
            return PeerOutcome::Stopped;
        }
        let label = endpoint_label(&ep);
        let params = SessionParams {
            identity: shared.identity,
            hello: shared.hello,
            storage: shared.storage,
            expected_device_id: Some(target_id),
            timeout: CONNECT_TIMEOUT,
            control: Some(Arc::clone(shared.control)),
        };
        let mut emit = |event: SyncEvent| log_event(shared.log, on_offer, event);
        let result = match ep {
            PeerEndpoint::Direct(addr) => connect_and_sync(&params, addr, &mut emit),
            PeerEndpoint::Relay { addr, device_id } => connect_via_relay(
                shared.identity,
                addr,
                device_id,
                target_id,
                GLOBAL_DISCOVERY_TIMEOUT,
            )
            .and_then(|tcp| connect_and_sync_stream(&params, tcp, addr, &mut emit)),
        };
        match result {
            Ok(hs) => match hs.session_status {
                SyncSessionStatus::Active {
                    files_downloaded,
                    bytes_downloaded,
                    blocks_uploaded,
                    bytes_uploaded,
                    ..
                } => {
                    return PeerOutcome::Synced(PeerStats {
                        files_downloaded,
                        bytes_downloaded,
                        blocks_uploaded,
                        bytes_uploaded,
                    })
                }
                SyncSessionStatus::AwaitingPairing => outcome = PeerOutcome::AwaitingPairing,
                SyncSessionStatus::ClosedByPeer(reason) => {
                    return PeerOutcome::ClosedByPeer(reason)
                }
            },
            Err(e) => shared.log.line(&format!(
                "connection to {} ({label}) failed: {e}",
                peer.name
            )),
        }
    }
    outcome
}

/// Logs the events worth a line (never per block or per up-to-date file).
fn log_event(log: &WorkerLog, on_offer: &mut OfferSink<'_>, event: SyncEvent) {
    match event {
        SyncEvent::PeerConnected {
            peer_addr,
            peer_name,
            ..
        } => log.line(&format!("connected to {peer_name} ({peer_addr})")),
        SyncEvent::FileSynced { folder, name, .. } => log.line(&format!("pulled {folder}/{name}")),
        SyncEvent::FileError {
            folder,
            name,
            error,
        } => log.line(&format!("cannot pull {folder}/{name}: {error}")),
        SyncEvent::LocalChangeApplied {
            folder,
            description,
        } => log.line(&format!("{folder}: {description}")),
        SyncEvent::FolderUnavailable { folder, error } => {
            log.line(&format!("folder {folder} unavailable: {error}"))
        }
        SyncEvent::ClusterConfigReceived {
            peer_device_id,
            folders,
        } => on_offer(peer_device_id, folders),
        SyncEvent::SessionEnded { reason, .. } => log.line(&format!("session ended: {reason}")),
        _ => {}
    }
}
