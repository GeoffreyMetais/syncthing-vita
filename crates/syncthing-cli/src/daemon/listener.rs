//! Accepts incoming connections (the PS Vita app, and any peer that dials us).

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;

use syncthing_core::accept_bep_tls_stream;

use super::context::DaemonContext;
use super::log::log;
use super::sessions::{run_session, Link, SessionEnd};
use super::timing::{CONNECT_TIMEOUT, STOP_CHECK_STEP};

/// Binds `addr`; the port is recorded in the context (it is announced by discovery).
pub fn bind(ctx: &DaemonContext, addr: SocketAddr) -> io::Result<TcpListener> {
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    *ctx.listen_port.lock().unwrap_or_else(|p| p.into_inner()) = Some(port);
    log(&format!("Listening on {}", listener.local_addr()?));
    Ok(listener)
}

/// Accepts connections until a stop is requested; each one is handled on its own thread.
pub fn run_listener(ctx: Arc<DaemonContext>, listener: TcpListener) {
    while !ctx.is_stopping() {
        match listener.accept() {
            Ok((tcp, addr)) => {
                let ctx = Arc::clone(&ctx);
                let spawned = std::thread::Builder::new()
                    .name("incoming".to_owned())
                    .spawn(move || handle_connection(&ctx, tcp, addr));
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

/// Authenticates the peer, then runs a session if it is a configured device.
fn handle_connection(ctx: &DaemonContext, tcp: TcpStream, addr: SocketAddr) {
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
        if ctx.first_report(format!("unknown:{device}")) {
            log(&format!(
                "Connection from unknown device {device} at {addr}. To accept it: syncthing-cli device add {device}"
            ));
        }
        return;
    };
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
                "{} ({addr}) closed the session: {reason}",
                peer.name
            ));
        }
        SessionEnd::Failed(e) => log(&format!("Session with {} ({addr}) failed: {e}", peer.name)),
    }
}
