//! Secondary BEP data connections (`1+2` multi-connection mode of Syncthing v2.x): opening,
//! waiting for promotion, serving block requests and closing.

use std::io;
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::context::SessionContext;
use super::upload::serve_block_request;
use crate::bep::{write_bep_message, BepClusterConfig, BepHello, BepMessageType};
use crate::device_id::DeviceId;
use crate::tls::client::open_bep_tls_stream;
use crate::tls::polling::try_read_bep_message;
use crate::tls::BepTlsStream;

/// Maximum number of secondary data connections attached per active peer session (`1+2`).
pub const MAX_SECONDARIES_PER_PEER: usize = 2;

/// Poll budget per secondary connection on each pass of the session loops.
const SECONDARY_POLL: Duration = Duration::from_millis(35);
/// The peer promotes a secondary connection after ~1 s; we wait at most this long for it.
const PROMOTION_BUDGET: Duration = Duration::from_millis(1400);
/// Minimum promotion wait, even when opening the connections already took long.
const MIN_PROMOTION_WAIT: Duration = Duration::from_millis(250);
/// Maximum slice of a single promotion poll.
const PROMOTION_POLL_SLICE: Duration = Duration::from_millis(350);
/// Timeout used when checking if an incoming duplicate connection is a secondary stream.
const INBOUND_SECONDARY_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// One secondary data connection.
pub struct SecondaryBepConn {
    pub stream: BepTlsStream,
    /// `true` once the peer sent its own secondary `ClusterConfig` (connection usable).
    pub promoted: bool,
}

/// Coordinates inbound secondary BEP connections (`ClusterConfig { secondary: true }`) between
/// an incoming TCP listener and the currently active primary session for a given peer.
#[derive(Debug, Default)]
pub struct SecondaryInbox {
    inboxes: Mutex<Vec<(DeviceId, Sender<BepTlsStream>)>>,
}

/// RAII registration of an active primary session in [`SecondaryInbox`]. Automatically
/// unregisters the peer on drop.
pub struct SecondaryInboxRegistration {
    inbox: Arc<SecondaryInbox>,
    peer_device_id: DeviceId,
}

impl Drop for SecondaryInboxRegistration {
    fn drop(&mut self) {
        self.inbox.unregister(&self.peer_device_id);
    }
}

impl SecondaryInbox {
    /// Creates an empty [`SecondaryInbox`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an active primary session for `peer_device_id` and returns the receiver for
    /// inbound secondary streams plus an RAII guard that unregisters on drop.
    pub fn register(
        self: &Arc<Self>,
        peer_device_id: DeviceId,
    ) -> (Receiver<BepTlsStream>, SecondaryInboxRegistration) {
        let (tx, rx) = mpsc::channel();
        let mut guard = self.inboxes.lock().unwrap_or_else(|p| p.into_inner());
        guard.retain(|(id, _)| !id.matches_peer(&peer_device_id));
        guard.push((peer_device_id, tx));
        (
            rx,
            SecondaryInboxRegistration {
                inbox: Arc::clone(self),
                peer_device_id,
            },
        )
    }

    /// Removes the registered channel for `peer_device_id`.
    pub fn unregister(&self, peer_device_id: &DeviceId) {
        let mut guard = self.inboxes.lock().unwrap_or_else(|p| p.into_inner());
        guard.retain(|(id, _)| !id.matches_peer(peer_device_id));
    }

    /// Returns `true` if `peer_device_id` currently has an active primary session accepting
    /// inbound secondary connections.
    pub fn has_active_session(&self, peer_device_id: &DeviceId) -> bool {
        let guard = self.inboxes.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .iter()
            .any(|(id, tx)| id.matches_peer(peer_device_id) && !is_disconnected(tx))
    }

    /// Delivers an authenticated secondary `BepTlsStream` (`ClusterConfig { secondary: true }`)
    /// to the active primary session for `peer_device_id`.
    /// Returns `Ok(())` on delivery, or `Err(Box::new(stream))` if no active session is registered.
    pub fn deliver(
        &self,
        peer_device_id: &DeviceId,
        stream: BepTlsStream,
    ) -> Result<(), Box<BepTlsStream>> {
        let sender = {
            let guard = self.inboxes.lock().unwrap_or_else(|p| p.into_inner());
            guard
                .iter()
                .find(|(id, _)| id.matches_peer(peer_device_id))
                .map(|(_, tx)| tx.clone())
        };
        match sender {
            Some(tx) => tx.send(stream).map_err(|e| Box::new(e.0)),
            None => Err(Box::new(stream)),
        }
    }
}

fn is_disconnected(tx: &Sender<BepTlsStream>) -> bool {
    // `mpsc::Sender` has no `is_closed()` in std without sending; an inactive receiver is
    // removed on drop via `SecondaryInboxRegistration` anyway.
    let _ = tx;
    false
}

/// Checks an already-TLS-authenticated incoming stream from a peer that already holds a primary
/// session: exchanges `Hello` (advertising `1+2` connections) and reads the first BEP message.
/// Returns `Ok(true)` if the peer sent `ClusterConfig { secondary: true }`, or `Ok(false)` if it
/// did not (primary/duplicate `ClusterConfig` or EOF).
pub fn accept_secondary_cluster_config(
    stream: &mut BepTlsStream,
    our_hello: &BepHello,
) -> io::Result<bool> {
    let mut hello = our_hello.clone();
    if hello.num_connections < 3 {
        hello.num_connections = 3;
    }
    let _peer_hello = hello.exchange(stream)?;
    let Some((msg_type, payload)) = try_read_bep_message(stream, INBOUND_SECONDARY_CHECK_TIMEOUT)?
    else {
        return Ok(false);
    };
    if msg_type != BepMessageType::ClusterConfig {
        return Ok(false);
    }
    let cfg = BepClusterConfig::decode(&payload)?;
    Ok(cfg.secondary)
}

/// Promotes any newly arrived inbound secondary streams from `rx` (up to `MAX_SECONDARIES_PER_PEER`
/// total in `secondaries`) by sending our `ClusterConfig { secondary: true }` reply.
/// Returns `true` if at least one new secondary connection was attached.
pub fn attach_inbound_secondaries(
    rx: Option<&Receiver<BepTlsStream>>,
    secondaries: &mut Vec<SecondaryBepConn>,
) -> bool {
    let Some(rx) = rx else {
        return false;
    };
    let mut attached = false;
    while let Ok(mut sec_stream) = rx.try_recv() {
        if secondaries.len() >= MAX_SECONDARIES_PER_PEER {
            drop(sec_stream);
            continue;
        }
        if write_bep_message(
            &mut sec_stream,
            BepMessageType::ClusterConfig,
            &BepClusterConfig::encode_secondary(),
        )
        .is_ok()
        {
            secondaries.push(SecondaryBepConn {
                stream: sec_stream,
                promoted: true,
            });
            attached = true;
        }
    }
    attached
}

/// Number of secondary connections to open, given both `Hello`s (`0..=2`).
pub fn desired_secondary_count(peer_hello: &BepHello, our_hello: &BepHello) -> usize {
    if peer_hello.num_connections > 1 {
        (peer_hello.num_connections.max(our_hello.num_connections) - 1).clamp(0, 2) as usize
    } else {
        0
    }
}

/// Opens a secondary BEP/1.0 data connection (`ClusterConfig { secondary: true }`).
pub fn open_secondary_bep_stream(
    ctx: &SessionContext<'_>,
    peer_addr: SocketAddr,
    timeout: Duration,
) -> io::Result<SecondaryBepConn> {
    let (mut stream, _) =
        open_bep_tls_stream(ctx.identity, peer_addr, Some(ctx.peer_device_id), timeout)?;
    let _ = ctx.hello.exchange(&mut stream)?;
    write_bep_message(
        &mut stream,
        BepMessageType::ClusterConfig,
        &BepClusterConfig::encode_secondary(),
    )?;
    Ok(SecondaryBepConn {
        stream,
        promoted: false,
    })
}

/// Drains all buffered messages (`Request`, `Ping`, secondary `ClusterConfig`) on a secondary stream.
pub fn poll_secondary_stream(
    sec: &mut SecondaryBepConn,
    poll_timeout: Duration,
    stop_when_promoted: bool,
    ctx: &mut SessionContext<'_>,
) -> io::Result<()> {
    loop {
        match try_read_bep_message(&mut sec.stream, poll_timeout)? {
            None => return Ok(()),
            Some((BepMessageType::ClusterConfig, _)) => {
                sec.promoted = true;
                if stop_when_promoted {
                    return Ok(());
                }
            }
            Some((BepMessageType::Request, payload)) => {
                serve_block_request(&mut sec.stream, ctx, &payload)?;
            }
            Some((BepMessageType::Ping, _)) => {
                write_bep_message(&mut sec.stream, BepMessageType::Ping, &[])?;
            }
            Some((BepMessageType::Close, _)) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Secondary connection closed by peer",
                ));
            }
            Some(_) => {}
        }
    }
}

/// Polls every secondary connection once, dropping those that failed.
pub fn poll_all_secondaries(secondaries: &mut Vec<SecondaryBepConn>, ctx: &mut SessionContext<'_>) {
    secondaries.retain_mut(|sec| poll_secondary_stream(sec, SECONDARY_POLL, false, ctx).is_ok());
}

/// Waits for the peer's ~1 s connection promotion timer on freshly opened secondary
/// connections, so the peer can dispatch block `Request`s over all `1+2` connections as soon
/// as our `Index` is published.
pub fn wait_for_promotion(
    secondaries: &mut [SecondaryBepConn],
    opened_at: Instant,
    ctx: &mut SessionContext<'_>,
) {
    if secondaries.is_empty() {
        return;
    }
    let budget = PROMOTION_BUDGET
        .saturating_sub(opened_at.elapsed())
        .max(MIN_PROMOTION_WAIT);
    let deadline = Instant::now() + budget;
    for sec in secondaries {
        while !sec.promoted && Instant::now() < deadline {
            let slice = deadline
                .saturating_duration_since(Instant::now())
                .min(PROMOTION_POLL_SLICE);
            if poll_secondary_stream(sec, slice, true, ctx).is_err() {
                break;
            }
        }
    }
}

/// Sends a `Ping` keepalive on every secondary connection, dropping those that failed.
pub fn ping_all_secondaries(secondaries: &mut Vec<SecondaryBepConn>) {
    secondaries
        .retain_mut(|sec| write_bep_message(&mut sec.stream, BepMessageType::Ping, &[]).is_ok());
}
