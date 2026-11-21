//! Bidirectional BEP/1.0 synchronization session with one Syncthing peer
//! (`1+2` multi-connection mode and optional persistent keepalive).
//!
//! The session runs in phases, one file each:
//! 1. TLS connection (dialed: [`connect_and_sync`], or accepted: [`accept_and_sync`]) +
//!    `Hello` exchange (this file, via [`crate::tls`])
//! 2. [`cluster_exchange`]: `ClusterConfig` exchange and initial remote `Index`
//! 3. [`secondary`]: `+2` secondary data connections
//! 4. [`initial_sync`]: initial download, local `Index` publication, request draining
//! 5. [`persistent`]: persistent loop (when a [`SyncControl`] is given)
//!
//! Public API: the entry points of this file with [`params`], [`control`], [`events`],
//! [`outcome`] and [`tuning`]. [`context`], [`folders`] and [`upload`] are public building
//! blocks (usable to serve requests on a custom transport); the phases are private.
//!
//! Shared building blocks: [`context`] (identity, storage, stats, event sink), [`folders`]
//! (per-session folder state), [`download`] / [`upload`] (block transfers), [`keepalive`],
//! [`params`] (what the application provides), [`tuning`] (timings it chooses),
//! [`index_exchange`] (local index publication, runtime `ClusterConfig` / `Index`) and
//! [`dispatch`] (primary-connection message routing).

mod cluster_exchange;
pub mod context;
pub mod control;
mod dispatch;
mod download;
pub mod events;
pub mod folders;
mod index_exchange;
mod initial_sync;
mod keepalive;
pub mod outcome;
mod override_changes;
pub mod params;
mod persistent;
mod revert;
mod secondary;
pub mod tuning;
pub mod upload;

use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use control::SyncControl;
pub use events::SyncEvent;
pub use outcome::{PeerHandshakeResult, SyncSessionStatus};
pub use params::SessionParams;
pub use secondary::{
    accept_secondary_cluster_config, SecondaryInbox, SecondaryInboxRegistration,
    MAX_SECONDARIES_PER_PEER,
};
pub use tuning::SyncTuning;

use cluster_exchange::{exchange_cluster_config, ClusterExchange};
use context::SessionContext;
use dispatch::with_context;
use folders::FolderSyncState;
use initial_sync::{run_initial_sync, InitialSync};
use persistent::{run_persistent_session, PersistentLinks};
use secondary::{
    attach_inbound_secondaries, desired_secondary_count, open_secondary_bep_stream,
    SecondaryBepConn,
};

use std::net::TcpStream;

use crate::tls::client::{open_bep_tls_stream, wrap_bep_tls_stream};
use crate::tls::verifier::ObservedDeviceId;
use crate::tls::{BepTlsStream, IncomingBepStream};

/// Timeout used when opening the initial secondary connections.
const SECONDARY_OPEN_TIMEOUT: Duration = Duration::from_millis(2000);

/// Connects to `peer_addr` (TLS 1.3 + BEP/1.0) and synchronizes the folders shared with that
/// peer:
/// 1. primary connection, plus up to 2 secondary data connections when both sides support
///    them (`1+2 connections` in Syncthing v2.x);
/// 2. delta-sync download with streaming disk writes;
/// 3. scan of the local folders and publication of the local `Index`;
/// 4. with [`SessionParams::control`], a persistent loop serving block requests, answering
///    `Ping`s, applying live `IndexUpdate`s and rescanning until a stop is requested or the
///    connection ends. Without it, the call returns after one sync cycle.
pub fn connect_and_sync<F>(
    params: &SessionParams<'_>,
    peer_addr: SocketAddr,
    on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    let (stream, observed_device_id) = open_bep_tls_stream(
        params.identity,
        peer_addr,
        params.expected_device_id,
        params.timeout,
    )
    .map_err(|e| with_context("TCP connect", e))?;
    run_bep_session(
        params,
        stream,
        observed_device_id,
        peer_addr,
        true,
        on_event,
    )
}

/// Same as [`connect_and_sync`] over an already-connected `TcpStream` (e.g. a Syncthing Relay
/// session tunnel). No secondary connection is opened: the tunnel is a single stream.
pub fn connect_and_sync_stream<F>(
    params: &SessionParams<'_>,
    tcp: TcpStream,
    peer_addr: SocketAddr,
    on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    let (stream, observed_device_id) = wrap_bep_tls_stream(
        params.identity,
        tcp,
        params.expected_device_id,
        params.timeout,
    )
    .map_err(|e| with_context("TLS wrap", e))?;
    run_bep_session(
        params,
        stream,
        observed_device_id,
        peer_addr,
        false,
        on_event,
    )
}

/// Same as [`connect_and_sync`] with a peer that connected to us: `incoming` comes from
/// [`crate::tls::accept_bep_tls_stream`] (a TCP listener, or a relay invitation with
/// `ServerSocket` set). [`SessionParams::expected_device_id`], when set, must match the peer.
///
/// When [`SessionParams::secondary_inbox`] is `Some`, our `Hello` advertises `1+2` connections
/// (`num_connections >= 3`) and registers the active session in the inbox so secondary
/// connections dialed by the peer can attach seamlessly. When `None` (e.g. single-stream relays),
/// our `Hello` announces a single connection so the peer opens none.
pub fn accept_and_sync<F>(
    params: &SessionParams<'_>,
    incoming: IncomingBepStream,
    peer_addr: SocketAddr,
    on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    let peer_device_id = incoming.peer_device_id();
    if let Some(expected) = params.expected_device_id {
        if !expected.matches_peer(&peer_device_id) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Peer TLS DeviceId ({peer_device_id}) != expected DeviceId ({expected})"),
            ));
        }
    }
    let mut hello = params.hello.clone();
    if params.secondary_inbox.is_some() {
        if hello.num_connections < 3 {
            hello.num_connections = 3;
        }
    } else {
        hello.num_connections = 1;
    }
    let adjusted = SessionParams {
        hello: &hello,
        ..params.clone()
    };
    let observed: ObservedDeviceId = Arc::new(Mutex::new(Some(peer_device_id)));
    run_bep_session(
        &adjusted,
        incoming.stream,
        observed,
        peer_addr,
        false,
        on_event,
    )
}

fn run_bep_session<F>(
    params: &SessionParams<'_>,
    mut stream: BepTlsStream,
    observed_device_id: ObservedDeviceId,
    peer_addr: SocketAddr,
    allow_secondary: bool,
    mut on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    let observed = || observed_device_id.lock().ok().and_then(|g| *g);
    let control = params.control.as_deref();

    // 1. `Hello` exchange on the primary connection
    let peer_hello = match params.hello.exchange(&mut stream) {
        Ok(hello) => hello,
        Err(e) => {
            // TLS succeeded but the peer refused BEP: it does not know this device yet.
            return match observed() {
                Some(peer_device_id) => Ok(PeerHandshakeResult {
                    peer_addr,
                    peer_device_id,
                    peer_hello: None,
                    session_status: SyncSessionStatus::AwaitingPairing,
                }),
                None => Err(e),
            };
        }
    };

    let peer_device_id =
        observed().ok_or_else(|| io::Error::other("Peer certificate was not observed"))?;
    let mut result = PeerHandshakeResult {
        peer_addr,
        peer_device_id,
        peer_hello: Some(peer_hello.clone()),
        session_status: SyncSessionStatus::AwaitingPairing,
    };

    // Register in SecondaryInbox (unregistered automatically when `_sec_reg` drops).
    let (inbound_sec_rx, _sec_reg) = match params.secondary_inbox.as_ref() {
        Some(inbox) => {
            let (rx, reg) = inbox.register(peer_device_id);
            (Some(rx), Some(reg))
        }
        None => (None, None),
    };

    let mut ctx = SessionContext::new(
        params.identity,
        params.hello,
        params.storage,
        result.peer_device_id,
        &mut on_event,
    );
    if let Some(ctrl) = control {
        ctx.set_control(ctrl);
    }
    let mut folders = FolderSyncState::default();

    // 2. `ClusterConfig` exchange and initial remote `Index`
    match exchange_cluster_config(&mut stream, &mut ctx, &mut folders, peer_addr, &peer_hello)? {
        ClusterExchange::Established => {}
        ClusterExchange::AwaitingPairing => return Ok(result),
        ClusterExchange::ClosedByPeer(reason) => {
            result.session_status = SyncSessionStatus::ClosedByPeer(reason);
            return Ok(result);
        }
    }

    // 3. `+2` secondary data connections, if the peer supports them and direct TCP is used
    let desired_secondary = if allow_secondary {
        desired_secondary_count(&peer_hello, params.hello)
    } else {
        0
    };
    let secondaries_opened_at = Instant::now();
    let mut secondaries: Vec<SecondaryBepConn> = (0..desired_secondary)
        .filter_map(|_| open_secondary_bep_stream(&ctx, peer_addr, SECONDARY_OPEN_TIMEOUT).ok())
        .collect();
    attach_inbound_secondaries(inbound_sec_rx.as_ref(), &mut secondaries);

    // 4. Initial download, local index publication and request draining
    let initial = run_initial_sync(
        &mut stream,
        &mut ctx,
        &mut folders,
        &mut secondaries,
        secondaries_opened_at,
        inbound_sec_rx.as_ref(),
        control.is_some(),
    )?;
    if let InitialSync::ClosedByPeer(reason) = initial {
        result.session_status = SyncSessionStatus::ClosedByPeer(reason);
        return Ok(result);
    }

    // 5. Persistent session loop
    if let Some(ctrl) = control {
        let links = PersistentLinks {
            peer_addr,
            secondaries,
            desired_secondary,
            inbound_sec_rx,
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
