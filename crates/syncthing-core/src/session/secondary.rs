//! Secondary BEP data connections (`1+2` multi-connection mode of Syncthing v2.x): opening,
//! waiting for promotion, serving block requests and closing.

use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::context::SessionContext;
use super::upload::serve_block_request;
use crate::bep::{write_bep_message, BepClusterConfig, BepHello, BepMessageType};
use crate::tls::client::open_bep_tls_stream;
use crate::tls::polling::try_read_bep_message;
use crate::tls::BepTlsStream;

/// Poll budget per secondary connection on each pass of the session loops.
const SECONDARY_POLL: Duration = Duration::from_millis(35);
/// The peer promotes a secondary connection after ~1 s; we wait at most this long for it.
const PROMOTION_BUDGET: Duration = Duration::from_millis(1400);
/// Minimum promotion wait, even when opening the connections already took long.
const MIN_PROMOTION_WAIT: Duration = Duration::from_millis(250);
/// Maximum slice of a single promotion poll.
const PROMOTION_POLL_SLICE: Duration = Duration::from_millis(350);

/// One secondary data connection.
pub struct SecondaryBepConn {
    pub stream: BepTlsStream,
    /// `true` once the peer sent its own secondary `ClusterConfig` (connection usable).
    pub promoted: bool,
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
    our_hello: &BepHello,
    timeout: Duration,
) -> io::Result<SecondaryBepConn> {
    let (mut stream, _) =
        open_bep_tls_stream(ctx.identity, peer_addr, Some(ctx.peer_device_id), timeout)?;
    let _ = our_hello.exchange(&mut stream)?;
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
