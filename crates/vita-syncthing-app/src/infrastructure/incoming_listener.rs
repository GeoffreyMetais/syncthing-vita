//! Incoming BEP/1.0 TCP listener (`0.0.0.0:22000` with ephemeral fallback).
//!
//! Accepts inbound TLS 1.3 connections from LAN or port-forwarded peers:
//! - Caps concurrent unauthenticated TLS handshakes via [`ConnectionSlots`].
//! - Rejects loopback self-connections before checking `approved_devices`.
//! - Reports unknown peers once (gated by [`UnknownDeviceLog`]) via [`WorkerMsg::DeviceOffered`]
//!   with IP-only address (never an ephemeral client port) and closes the stream without sending
//!   `Hello` or `ClusterConfig`.
//! - Deduplicates approved peers via [`SessionClaims`] and runs [`accept_and_sync`], forwarding
//!   events as [`WorkerMsg::InboundSyncEvent`].
//! - Reaps finished handler threads continuously and joins all remaining handles before returning.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use syncthing_core::tls::polling::is_socket_timeout_or_would_block;
use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, accept_secondary_cluster_config, BepHello,
    DeviceIdentity, FsSyncStorage, SecondaryInbox, SessionParams, SyncControl, SyncEvent,
    SyncSessionStatus,
};

use super::peer_endpoints::sleep_interruptible;
use super::session_guard::{
    ConnectionSlot, ConnectionSlots, SessionClaims, UnknownDeviceLog, UnknownReport,
};
use super::sync_worker::{WorkerMsg, WORKER_STACK_SIZE};
use crate::domain::defaults::{CONNECT_TIMEOUT, INCOMING_ACCEPT_POLL};
use crate::domain::KnownDevice;

/// Shared context passed to [`run_tcp_listener`] by `spawn_sync_worker` (or tests).
#[derive(Clone)]
pub struct IncomingListenerContext {
    pub identity: DeviceIdentity,
    pub hello: BepHello,
    pub storage: FsSyncStorage,
    pub approved_devices: Vec<KnownDevice>,
    pub claims: Arc<SessionClaims>,
    pub secondary_inbox: Arc<SecondaryInbox>,
    pub slots: Arc<ConnectionSlots>,
    pub unknown_log: Arc<UnknownDeviceLog>,
    pub control: Arc<SyncControl>,
    pub tx: Sender<WorkerMsg>,
}

/// Binds an IPv4 non-blocking TCP listener on `0.0.0.0:preferred_port`, falling back to
/// `0.0.0.0:0` if `preferred_port` is non-zero and unavailable. Returns `(listener, bound_port)`.
pub fn bind_bep_listener(preferred_port: u16) -> io::Result<(TcpListener, u16)> {
    let listener = match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], preferred_port))) {
        Ok(l) => l,
        Err(_) if preferred_port != 0 => TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))?,
        Err(e) => return Err(e),
    };
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// Accepts incoming BEP/1.0 TCP connections until `ctx.control.is_stop_requested()`.
/// Joins all spawned `bep_in` handler threads before returning.
pub fn run_tcp_listener(listener: TcpListener, ctx: IncomingListenerContext) {
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    let mut busy_logged = false;
    let mut last_accept_err = String::new();

    while !ctx.control.is_stop_requested() {
        reap_finished_handles(&mut handles);

        match listener.accept() {
            Ok((tcp, peer_addr)) => {
                last_accept_err.clear();
                let Some(slot) = ctx.slots.try_take() else {
                    if !busy_logged {
                        busy_logged = true;
                        let _ = ctx.tx.send(WorkerMsg::Log(format!(
                            "Too many incoming handshakes in progress; dropping connection from {}.",
                            peer_addr.ip()
                        )));
                    }
                    drop(tcp);
                    continue;
                };
                busy_logged = false;

                let conn_ctx = ctx.clone();
                match thread::Builder::new()
                    .name("bep_in".to_owned())
                    .stack_size(WORKER_STACK_SIZE)
                    .spawn(move || handle_incoming_tcp(conn_ctx, tcp, peer_addr, slot))
                {
                    Ok(handle) => handles.push(handle),
                    Err(e) => {
                        let _ = ctx.tx.send(WorkerMsg::Log(format!(
                            "Cannot spawn incoming BEP handler for {}: {e}",
                            peer_addr.ip()
                        )));
                    }
                }
            }
            Err(e) if is_socket_timeout_or_would_block(&e) => {
                if !sleep_interruptible(&ctx.control, INCOMING_ACCEPT_POLL) {
                    break;
                }
            }
            Err(e) => {
                let msg = format!("Incoming BEP listener accept error: {e}");
                if msg != last_accept_err {
                    let _ = ctx.tx.send(WorkerMsg::Log(msg.clone()));
                    last_accept_err = msg;
                }
                if !sleep_interruptible(&ctx.control, INCOMING_ACCEPT_POLL) {
                    break;
                }
            }
        }
    }

    for handle in handles {
        let _ = handle.join();
    }
}

fn reap_finished_handles(handles: &mut Vec<JoinHandle<()>>) {
    let mut i = 0;
    while i < handles.len() {
        if handles[i].is_finished() {
            let _ = handles.swap_remove(i).join();
        } else {
            i += 1;
        }
    }
}

fn handle_incoming_tcp(
    ctx: IncomingListenerContext,
    tcp: TcpStream,
    peer_addr: SocketAddr,
    slot: ConnectionSlot,
) {
    if tcp.set_nonblocking(false).is_err() {
        return;
    }

    let stream = match accept_bep_tls_stream(&ctx.identity, tcp, CONNECT_TIMEOUT) {
        Ok(s) => s,
        Err(e) => {
            if !ctx.control.is_stop_requested() {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Incoming TLS handshake from {} failed: {e}",
                    peer_addr.ip()
                )));
            }
            return;
        }
    };

    let peer_id = stream.peer_device_id();
    if peer_id == ctx.identity.device_id {
        let _ = ctx.tx.send(WorkerMsg::Log(format!(
            "Ignored incoming self-connection from {}.",
            peer_addr.ip()
        )));
        return;
    }

    let Some(kd) = ctx
        .approved_devices
        .iter()
        .find(|kd| kd.device_id.matches_peer(&peer_id))
    else {
        match ctx.unknown_log.record(peer_id) {
            UnknownReport::First => {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Incoming TCP: unknown device {} from {} wants to connect.",
                    peer_id.short(),
                    peer_addr.ip()
                )));
                let _ = ctx.tx.send(WorkerMsg::DeviceOffered {
                    device_id: peer_id,
                    address: peer_addr.ip().to_string(),
                    via_relay: false,
                });
            }
            UnknownReport::LimitReached => {
                let _ = ctx.tx.send(WorkerMsg::Log(
                    "Incoming TCP: unknown device report limit reached; suppressing further reports."
                        .to_owned(),
                ));
            }
            UnknownReport::Quiet => {}
        }
        return;
    };

    // Release the handshake slot before claiming a long-lived session or probing secondary status.
    drop(slot);

    let Some(_claim) = ctx.claims.try_claim(peer_id) else {
        if ctx.secondary_inbox.has_active_session(&peer_id) {
            let mut tls_stream = stream.into_stream();
            match accept_secondary_cluster_config(&mut tls_stream, &ctx.hello) {
                Ok(true) => {
                    if ctx.secondary_inbox.deliver(&peer_id, tls_stream).is_ok() {
                        return;
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    let _ = ctx.tx.send(WorkerMsg::Log(format!(
                        "Incoming secondary check from {} ({}) failed: {e}",
                        kd.name,
                        peer_addr.ip()
                    )));
                    return;
                }
            }
        }
        let _ = ctx.tx.send(WorkerMsg::Log(format!(
            "Ignored duplicate incoming connection from {} ({}).",
            kd.name,
            peer_addr.ip()
        )));
        return;
    };

    let params = SessionParams {
        identity: &ctx.identity,
        hello: &ctx.hello,
        storage: &ctx.storage,
        expected_device_id: Some(peer_id),
        timeout: CONNECT_TIMEOUT,
        control: Some(Arc::clone(&ctx.control)),
        secondary_inbox: Some(Arc::clone(&ctx.secondary_inbox)),
    };

    let result = accept_and_sync(&params, stream, peer_addr, |ev| {
        if let SyncEvent::PeerConnected { peer_device_id, .. } = &ev {
            ctx.claims.mark_established(peer_device_id);
        }
        let _ = ctx.tx.send(WorkerMsg::InboundSyncEvent(ev));
    });

    if ctx.control.is_stop_requested() {
        return;
    }

    match result {
        Ok(hs) => match hs.session_status {
            SyncSessionStatus::Active { .. } => {}
            SyncSessionStatus::AwaitingPairing => {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Incoming peer {} ({}) awaiting device acceptance.",
                    kd.name,
                    peer_addr.ip()
                )));
            }
            SyncSessionStatus::ClosedByPeer(reason) => {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Incoming session with {} ({}) closed by peer: {}",
                    kd.name,
                    peer_addr.ip(),
                    reason
                )));
            }
        },
        Err(e) => {
            let _ = ctx.tx.send(WorkerMsg::Log(format!(
                "Incoming session with {} ({}) failed: {e}",
                kd.name,
                peer_addr.ip()
            )));
        }
    }
}
