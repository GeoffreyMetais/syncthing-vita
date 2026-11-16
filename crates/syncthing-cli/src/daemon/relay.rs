//! Relay mode: joins a relay so that devices which cannot dial us directly (NAT, firewall)
//! reach us through it, and runs the sessions it invites us to.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use syncthing_core::discovery::PeerEndpoint;
use syncthing_core::{
    accept_bep_tls_stream, fetch_relay_pool, join_relay_session, resolve_endpoint_uri, DeviceId,
    HttpsServer, RelayListener, SessionInvitation,
};

use super::connection_slots::{ConnectionSlot, ConnectionSlots};
use super::context::DaemonContext;
use super::log::log;
use super::sessions::{run_session, Link, SessionEnd};
use super::timing::{
    CONNECT_TIMEOUT, HTTPS_TIMEOUT, MAX_PENDING_RELAY_INVITATIONS, RELAY_POLL, RELAY_POOL_RETRY,
    RELAY_RETRY,
};

/// What to do with a relay invitation, decided on the relay's thread before any thread is
/// spent on it. Its `from` is only what the relay claims: the TLS handshake proves it.
#[derive(Debug)]
pub enum InvitationTriage {
    /// A configured device without a session: join the relay session on a thread holding
    /// this slot until the session is about to run.
    Join(DeviceId, ConnectionSlot),
    /// `from` is not a device ID.
    Malformed,
    /// Not a configured device: reported, never joined.
    UnknownDevice(DeviceId),
    /// The device already has a session: joining would only end in `AlreadyConnected`.
    AlreadyConnected(DeviceId),
    /// Every slot is taken by invitations still being joined: dropped.
    Busy(DeviceId),
}

/// Triages an invitation from `from`: the cheap rejections first, then a slot of `slots`.
pub fn triage_invitation(
    from: &[u8],
    is_configured: impl FnOnce(&DeviceId) -> bool,
    is_claimed: impl FnOnce(&DeviceId) -> bool,
    slots: &ConnectionSlots,
) -> InvitationTriage {
    let Ok(raw_id) = <[u8; 32]>::try_from(from) else {
        return InvitationTriage::Malformed;
    };
    let id = DeviceId(raw_id);
    if !is_configured(&id) {
        return InvitationTriage::UnknownDevice(id);
    }
    if is_claimed(&id) {
        return InvitationTriage::AlreadyConnected(id);
    }
    match slots.try_take() {
        Some(slot) => InvitationTriage::Join(id, slot),
        None => InvitationTriage::Busy(id),
    }
}

/// The relays to try, in order: the configured ones, or the public pool in a random order.
fn candidate_relays(ctx: &DaemonContext) -> Vec<String> {
    if !ctx.options.static_relays.is_empty() {
        return ctx.options.static_relays.clone();
    }
    let Some(pool) = HttpsServer::parse(&ctx.options.relay_pool_url) else {
        log(&format!(
            "Invalid relay pool URL {}",
            ctx.options.relay_pool_url
        ));
        return Vec::new();
    };
    match fetch_relay_pool(&pool, &ctx.options.https, HTTPS_TIMEOUT) {
        Ok(mut relays) => {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as usize)
                .unwrap_or(0);
            if !relays.is_empty() {
                let shift = seed % relays.len();
                relays.rotate_left(shift);
            }
            relays
        }
        Err(e) => {
            log(&format!("Cannot fetch the relay pool {}: {e}", pool.host));
            Vec::new()
        }
    }
}

/// Keeps us joined to a relay until a stop is requested. At most
/// [`MAX_PENDING_RELAY_INVITATIONS`] invitations are being joined at a time, whichever relay
/// they came from.
pub fn run_relay(ctx: Arc<DaemonContext>) {
    let slots = ConnectionSlots::new(MAX_PENDING_RELAY_INVITATIONS);
    while !ctx.is_stopping() {
        let relays = candidate_relays(&ctx);
        if relays.is_empty() {
            ctx.sleep(RELAY_POOL_RETRY);
            continue;
        }
        for uri in relays {
            if ctx.is_stopping() {
                return;
            }
            let Some(PeerEndpoint::Relay { addr, device_id }) = resolve_endpoint_uri(&uri) else {
                continue;
            };
            match RelayListener::join(&ctx.identity, addr, device_id, CONNECT_TIMEOUT) {
                Ok(listener) => serve_relay(&ctx, listener, &slots),
                Err(e) => log(&format!("Cannot join relay {addr}: {e}")),
            }
            if !ctx.sleep(RELAY_RETRY) {
                return;
            }
        }
    }
}

/// Waits for invitations on a joined relay until it is lost or a stop is requested.
fn serve_relay(ctx: &Arc<DaemonContext>, mut listener: RelayListener, slots: &ConnectionSlots) {
    log(&format!(
        "Reachable through relay {}",
        listener.relay_addr()
    ));
    ctx.table().set_relay_uris(vec![listener.relay_uri()]);
    while !ctx.is_stopping() {
        match listener.poll_invitation(RELAY_POLL) {
            Ok(None) => {}
            Ok(Some(invitation)) => {
                accept_invitation(ctx, listener.relay_addr(), invitation, slots)
            }
            Err(e) => {
                log(&format!("Lost relay {}: {e}", listener.relay_addr()));
                break;
            }
        }
    }
    ctx.table().set_relay_uris(Vec::new());
}

/// Spends a thread on `invitation` only if it may become a session (see [`triage_invitation`]).
fn accept_invitation(
    ctx: &Arc<DaemonContext>,
    relay_addr: SocketAddr,
    invitation: SessionInvitation,
    slots: &ConnectionSlots,
) {
    let triage = triage_invitation(
        &invitation.from,
        |id| ctx.peer(id).is_some(),
        |id| ctx.table().is_claimed(id),
        slots,
    );
    let (from, slot) = match triage {
        InvitationTriage::Join(from, slot) => (from, slot),
        InvitationTriage::Malformed | InvitationTriage::AlreadyConnected(_) => return,
        InvitationTriage::UnknownDevice(from) => {
            ctx.report_unknown_device("relay", from, || {
                format!(
                    "Relay: unknown device {from} wants to connect. To accept it: syncthing-cli device add {from}"
                )
            });
            return;
        }
        InvitationTriage::Busy(from) => {
            if ctx.first_report("relay", "too-many-invitations") {
                log(&format!(
                    "Too many relay invitations in progress: dropping new ones (from {from})"
                ));
            }
            return;
        }
    };
    let thread_ctx = Arc::clone(ctx);
    let spawned = std::thread::Builder::new()
        .name("relay-session".to_owned())
        .spawn(move || handle_invitation(&thread_ctx, relay_addr, invitation, from, slot));
    if let Err(e) = spawned {
        log(&format!(
            "Cannot handle the relay invitation from {from}: {e}"
        ));
    }
}

/// Joins the session the configured device `from` requested through the relay (`slot` is
/// given back once the session is about to run, or on any earlier exit).
fn handle_invitation(
    ctx: &DaemonContext,
    relay_addr: SocketAddr,
    invitation: SessionInvitation,
    from: DeviceId,
    slot: ConnectionSlot,
) {
    let Some(peer) = ctx.peer(&from) else {
        return;
    };
    let tcp = match join_relay_session(relay_addr, &invitation, CONNECT_TIMEOUT) {
        Ok(tcp) => tcp,
        Err(e) => {
            log(&format!("Relay session with {} failed: {e}", peer.name));
            return;
        }
    };
    let link = if invitation.server_socket {
        match accept_bep_tls_stream(&ctx.identity, tcp, CONNECT_TIMEOUT) {
            Ok(stream) if stream.peer_device_id() != from => {
                if ctx.first_report(&format!("relay-impersonated:{from}"), "") {
                    log(&format!(
                        "Relay session announced as {} came from another device: dropped",
                        peer.name
                    ));
                }
                return;
            }
            Ok(stream) => Link::Incoming {
                stream: Box::new(stream),
                addr: relay_addr,
            },
            Err(e) => {
                log(&format!(
                    "Relay TLS handshake with {} failed: {e}",
                    peer.name
                ));
                return;
            }
        }
    } else {
        Link::RelayClient {
            tcp,
            addr: relay_addr,
        }
    };
    drop(slot);
    match run_session(ctx, peer, link) {
        SessionEnd::Ran | SessionEnd::AlreadyConnected => {}
        SessionEnd::NotAccepted(reason) => {
            log(&format!(
                "{} (relayed) did not accept us: {reason}",
                peer.name
            ));
        }
        SessionEnd::ClosedByPeer(reason) => {
            log(&format!(
                "{} (relayed) closed the session: {reason}",
                peer.name
            ));
        }
        SessionEnd::Failed(e) => log(&format!("Relayed session with {} failed: {e}", peer.name)),
    }
}
