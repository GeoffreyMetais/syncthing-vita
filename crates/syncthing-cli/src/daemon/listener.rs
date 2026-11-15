//! Accepts incoming connections (the PS Vita app, and any peer that dials us).

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use syncthing_core::accept_bep_tls_stream;

use super::context::DaemonContext;
use super::log::log;
use super::sessions::{run_session, Link, SessionEnd};
use super::timing::{CONNECT_TIMEOUT, MAX_UNAUTHENTICATED_CONNECTIONS, STOP_CHECK_STEP};

/// Binds `addr`; the port is recorded in the context (it is announced by discovery).
pub fn bind(ctx: &DaemonContext, addr: SocketAddr) -> io::Result<TcpListener> {
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    *ctx.listen_port.lock().unwrap_or_else(|p| p.into_inner()) = Some(port);
    log(&format!("Listening on {}", listener.local_addr()?));
    Ok(listener)
}

/// A bounded number of slots, each held by one connection until it is dropped.
#[derive(Debug, Clone)]
pub struct ConnectionSlots {
    in_use: Arc<AtomicUsize>,
    capacity: usize,
}

/// One slot of [`ConnectionSlots`], given back when dropped (on every exit path).
#[derive(Debug)]
pub struct ConnectionSlot {
    in_use: Arc<AtomicUsize>,
}

impl ConnectionSlots {
    pub fn new(capacity: usize) -> Self {
        Self {
            in_use: Arc::new(AtomicUsize::new(0)),
            capacity,
        }
    }

    /// Takes a slot; `None` when all of them are in use.
    pub fn try_take(&self) -> Option<ConnectionSlot> {
        self.in_use
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.capacity).then_some(n + 1)
            })
            .ok()
            .map(|_| ConnectionSlot {
                in_use: Arc::clone(&self.in_use),
            })
    }

    /// Slots currently taken.
    pub fn in_use(&self) -> usize {
        self.in_use.load(Ordering::Acquire)
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.in_use.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Accepts connections until a stop is requested; each one is handled on its own thread.
/// At most [`MAX_UNAUTHENTICATED_CONNECTIONS`] connections may be in their TLS handshake (or
/// being turned away as unknown devices) at a time: beyond, a new connection is closed at
/// once. A configured device's session no longer counts once authenticated.
pub fn run_listener(ctx: Arc<DaemonContext>, listener: TcpListener) {
    let slots = ConnectionSlots::new(MAX_UNAUTHENTICATED_CONNECTIONS);
    while !ctx.is_stopping() {
        match listener.accept() {
            Ok((tcp, addr)) => {
                let Some(slot) = slots.try_take() else {
                    if ctx.first_report("listener", "too-many-unauthenticated") {
                        log(&format!(
                            "Too many connections in progress: closing new ones (from {addr})"
                        ));
                    }
                    drop(tcp);
                    continue;
                };
                let ctx = Arc::clone(&ctx);
                let spawned = std::thread::Builder::new()
                    .name("incoming".to_owned())
                    .spawn(move || handle_connection(&ctx, tcp, addr, slot));
                if let Err(e) = spawned {
                    log(&format!("Cannot handle the connection from {addr}: {e}"));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(STOP_CHECK_STEP);
            }
            Err(e) => {
                log(&format!("Accept failed: {e}"));
                std::thread::sleep(STOP_CHECK_STEP);
            }
        }
    }
}

/// Authenticates the peer, then runs a session if it is a configured device (`slot` is given
/// back as soon as the peer is known to be one).
fn handle_connection(ctx: &DaemonContext, tcp: TcpStream, addr: SocketAddr, slot: ConnectionSlot) {
    if tcp.set_nonblocking(false).is_err() {
        return;
    }
    let stream = match accept_bep_tls_stream(&ctx.identity, tcp, CONNECT_TIMEOUT) {
        Ok(stream) => stream,
        Err(e) => {
            log(&format!("TLS handshake with {addr} failed: {e}"));
            return;
        }
    };
    let device = stream.peer_device_id();
    let Some(peer) = ctx.peer(&device) else {
        ctx.report_unknown_device("tcp", device, || {
            format!(
                "Connection from unknown device {device} at {addr}. To accept it: syncthing-cli device add {device}"
            )
        });
        return;
    };
    drop(slot);
    match run_session(
        ctx,
        peer,
        Link::Incoming {
            stream: Box::new(stream),
            addr,
        },
    ) {
        SessionEnd::Ran | SessionEnd::AlreadyConnected => {}
        SessionEnd::NotAccepted(reason) => {
            log(&format!(
                "{} ({addr}) did not accept us: {reason}",
                peer.name
            ));
        }
        SessionEnd::ClosedByPeer(reason) => {
            log(&format!(
                "{} ({addr}) closed the session: {reason}",
                peer.name
            ));
        }
        SessionEnd::Failed(e) => log(&format!("Session with {} ({addr}) failed: {e}", peer.name)),
    }
}
