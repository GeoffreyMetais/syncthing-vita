//! Running a BEP session with a configured device over any link (dialed TCP, accepted TCP,
//! relay in either direction), one session per device at a time.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

use syncthing_core::{
    accept_and_sync, connect_and_sync, connect_and_sync_stream, IncomingBepStream,
    PeerHandshakeResult, SessionParams, SyncSessionStatus,
};

use super::context::{DaemonContext, KnownPeer};
use super::events::handle_event;
use super::timing::CONNECT_TIMEOUT;

/// How a session reaches its peer.
pub enum Link {
    /// We dial `addr`.
    Dial(SocketAddr),
    /// We joined a relay session as the TLS client (`addr` is the relay's).
    RelayClient { tcp: TcpStream, addr: SocketAddr },
    /// The peer connected to us (TCP listener, or relay session as the TLS server).
    Incoming {
        stream: Box<IncomingBepStream>,
        addr: SocketAddr,
    },
}

/// How a session ended, to choose the retry delay.
#[derive(Debug)]
pub enum SessionEnd {
    /// The session ran (and ended: stop, connection lost).
    Ran,
    /// The peer does not accept us yet (it answered no `Hello` or no `ClusterConfig`).
    NotAccepted(String),
    /// The peer closed the session with this reason (e.g. it is shutting down) before the
    /// session was fully established.
    ClosedByPeer(String),
    /// Another session with this device already runs.
    AlreadyConnected,
    Failed(io::Error),
}

/// Runs a session with `peer` over `link` until it ends; the device's session slot is held
/// meanwhile, so a second link to the same device is dropped.
pub fn run_session(ctx: &DaemonContext, peer: &KnownPeer, link: Link) -> SessionEnd {
    if !ctx.table().claim(peer.id) {
        return SessionEnd::AlreadyConnected;
    }
    let result = run_claimed(ctx, peer, link);
    ctx.table().release(peer.id);
    match result {
        Ok(hs) => match hs.session_status {
            SyncSessionStatus::Active { .. } => SessionEnd::Ran,
            SyncSessionStatus::AwaitingPairing => {
                SessionEnd::NotAccepted("this device is not accepted by the peer yet".to_owned())
            }
            SyncSessionStatus::ClosedByPeer(reason) => SessionEnd::ClosedByPeer(reason),
        },
        Err(e) => SessionEnd::Failed(e),
    }
}

fn run_claimed(
    ctx: &DaemonContext,
    peer: &KnownPeer,
    link: Link,
) -> io::Result<PeerHandshakeResult> {
    let secondary_inbox = match &link {
        Link::Incoming { .. } => Some(Arc::clone(&ctx.secondary_inbox)),
        Link::Dial(_) | Link::RelayClient { .. } => None,
    };
    let params = SessionParams {
        identity: &ctx.identity,
        hello: &ctx.hello,
        storage: &ctx.storage,
        expected_device_id: Some(peer.id),
        timeout: CONNECT_TIMEOUT,
        control: Some(Arc::clone(&ctx.control)),
        secondary_inbox,
    };
    let on_event = |event| handle_event(ctx, peer, event);
    match link {
        Link::Dial(addr) => connect_and_sync(&params, addr, on_event),
        Link::RelayClient { tcp, addr } => connect_and_sync_stream(&params, tcp, addr, on_event),
        Link::Incoming { stream, addr } => accept_and_sync(&params, *stream, addr, on_event),
    }
}
