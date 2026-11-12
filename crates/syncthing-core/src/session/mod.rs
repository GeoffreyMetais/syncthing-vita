//! Bidirectional BEP/1.0 synchronization session with one Syncthing peer
//! (`1+2` multi-connection mode and optional persistent keepalive).
//!
//! The session runs in phases, one file each:
//! 1. TLS connection + `Hello` exchange (this file, via [`crate::tls`])
//! 2. [`cluster_exchange`]: `ClusterConfig` exchange and initial remote `Index`
//! 3. [`secondary`]: `+2` secondary data connections
//! 4. [`initial_sync`]: initial download, local `Index` publication, request draining
//! 5. [`persistent`]: persistent loop (when a [`SyncControl`] is given)
//!
//! Shared building blocks: [`context`] (identity, storage, stats, event sink), [`folders`]
//! (per-session folder state), [`download`] / [`upload`] (block transfers), [`keepalive`],
//! [`tuning`] (timings chosen by the application),
//! [`index_exchange`] (local index publication, runtime `ClusterConfig` / `Index`) and
//! [`dispatch`] (primary-connection message routing).

pub mod cluster_exchange;
pub mod context;
pub mod control;
pub mod dispatch;
pub mod download;
pub mod events;
pub mod folders;
pub mod index_exchange;
pub mod initial_sync;
pub mod keepalive;
pub mod outcome;
pub mod persistent;
pub mod secondary;
pub mod tuning;
pub mod upload;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use control::SyncControl;
pub use events::SyncEvent;
pub use outcome::{PeerHandshakeResult, SyncSessionStatus};
pub use tuning::SyncTuning;

use cluster_exchange::{exchange_cluster_config, ClusterExchange};
use context::SessionContext;
use dispatch::with_context;
use folders::FolderSyncState;
use initial_sync::{run_initial_sync, InitialSync};
use persistent::{run_persistent_session, PersistentLinks};
use secondary::{desired_secondary_count, open_secondary_bep_stream, SecondaryBepConn};

use crate::bep::BepHello;
use crate::cert::VitaIdentity;
use crate::device_id::DeviceId;
use crate::storage::FsSyncStorage;
use crate::tls::client::open_bep_tls_stream;

/// Timeout used when opening the initial secondary connections.
const SECONDARY_OPEN_TIMEOUT: Duration = Duration::from_millis(2000);

/// Establishes a single-cycle TLS 1.3 + BEP/1.0 session without persistent background keepalive.
pub fn connect_and_sync<F>(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    our_hello: &BepHello,
    storage: &FsSyncStorage,
    timeout: Duration,
    on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    connect_and_sync_with_control(
        identity,
        peer_addr,
        expected_device_id,
        our_hello,
        storage,
        timeout,
        None,
        on_event,
    )
}

/// Establishes TLS 1.3 + BEP/1.0 `1+2` multi-connection session and executes:
/// 1. Primary metadata connection + 2 secondary data connections (`1+2 connections` in Syncthing v2.x).
/// 2. Delta Sync download with 128 KiB streaming disk writes.
/// 3. Local indexing of target folder and publishing Vita's local `Index`.
/// 4. Persistent session loop (when `control` is `Some`), serving block requests, answering `Ping`s,
///    applying live remote `IndexUpdate`s, and rescanning local folders without disconnecting.
#[allow(clippy::too_many_arguments)]
pub fn connect_and_sync_with_control<F>(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    our_hello: &BepHello,
    storage: &FsSyncStorage,
    timeout: Duration,
    control: Option<Arc<SyncControl>>,
    mut on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    let (mut stream, observed_device_id) =
        open_bep_tls_stream(identity, peer_addr, expected_device_id, timeout)
            .map_err(|e| with_context("TCP connect", e))?;
    let observed = || observed_device_id.lock().ok().and_then(|g| *g);

    // 1. `Hello` exchange on the primary connection
    let peer_hello = match our_hello.exchange(&mut stream) {
        Ok(hello) => hello,
        Err(e) => {
            // TLS succeeded but the peer refused BEP: it does not know this device yet.
            return match observed() {
                Some(peer_device_id) => {
                    Ok(awaiting_pairing_without_hello(peer_addr, peer_device_id))
                }
                None => Err(e),
            };
        }
    };

    let mut result = PeerHandshakeResult {
        peer_addr,
        peer_device_id: observed()
            .ok_or_else(|| io::Error::other("Peer certificate was not observed"))?,
        peer_hello,
        tls_version: stream
            .conn
            .protocol_version()
            .map(|v| format!("{v:?}"))
            .unwrap_or_else(|| "Unknown".to_owned()),
        alpn: stream
            .conn
            .alpn_protocol()
            .map(|p| String::from_utf8_lossy(p).into_owned()),
        session_status: SyncSessionStatus::AwaitingPairing,
    };

    let mut ctx = SessionContext::new(identity, storage, result.peer_device_id, &mut on_event);
    if let Some(ctrl) = control.as_deref() {
        ctx.set_control(ctrl);
    }
    let mut folders = FolderSyncState::default();

    // 2. `ClusterConfig` exchange and initial remote `Index`
    match exchange_cluster_config(
        &mut stream,
        &mut ctx,
        &mut folders,
        peer_addr,
        &result.peer_hello,
    )? {
        ClusterExchange::Established => {}
        ClusterExchange::AwaitingPairing => return Ok(result),
        ClusterExchange::ClosedByPeer(reason) => {
            result.session_status = SyncSessionStatus::ClosedByPeer(reason);
            return Ok(result);
        }
    }

    // 3. `+2` secondary data connections, if the peer supports them
    let desired_secondary = desired_secondary_count(&result.peer_hello, our_hello);
    let secondaries_opened_at = Instant::now();
    let mut secondaries: Vec<SecondaryBepConn> = (0..desired_secondary)
        .filter_map(|_| {
            open_secondary_bep_stream(&ctx, peer_addr, our_hello, SECONDARY_OPEN_TIMEOUT).ok()
        })
        .collect();

    // 4. Initial download, local index publication and request draining
    let initial = run_initial_sync(
        &mut stream,
        &mut ctx,
        &mut folders,
        &mut secondaries,
        secondaries_opened_at,
        control.is_some(),
    )?;
    if let InitialSync::ClosedByPeer(reason) = initial {
        result.session_status = SyncSessionStatus::ClosedByPeer(reason);
        return Ok(result);
    }

    // 5. Persistent session loop
    if let Some(ctrl) = control.as_deref() {
        let links = PersistentLinks {
            peer_addr,
            our_hello,
            secondaries,
            desired_secondary,
        };
        run_persistent_session(&mut stream, &mut ctx, &mut folders, links, ctrl);
    }

    let stats = ctx.stats;
    result.session_status = SyncSessionStatus::Active {
        shared_folders: folders.shared_folders,
        files_up_to_date: stats.files_up_to_date,
        files_downloaded: stats.files_downloaded,
        bytes_downloaded: stats.bytes_downloaded,
        blocks_uploaded: stats.blocks_uploaded,
        bytes_uploaded: stats.bytes_uploaded,
    };
    Ok(result)
}

/// Result reported when TLS succeeded but the peer refused the BEP `Hello`.
fn awaiting_pairing_without_hello(
    peer_addr: SocketAddr,
    peer_device_id: DeviceId,
) -> PeerHandshakeResult {
    PeerHandshakeResult {
        peer_addr,
        peer_device_id,
        peer_hello: BepHello {
            device_name: format!("Syncthing ({})", peer_addr.ip()),
            client_name: "syncthing".to_owned(),
            client_version: "TLS 1.3".to_owned(),
            num_connections: 0,
            timestamp: 0,
        },
        tls_version: "TLSv1_3".to_owned(),
        alpn: Some("bep/1.0".to_owned()),
        session_status: SyncSessionStatus::AwaitingPairing,
    }
}
