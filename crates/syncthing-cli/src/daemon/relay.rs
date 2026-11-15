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

use super::context::DaemonContext;
use super::log::log;
use super::sessions::{run_session, Link, SessionEnd};
use super::timing::{CONNECT_TIMEOUT, HTTPS_TIMEOUT, RELAY_POLL, RELAY_POOL_RETRY, RELAY_RETRY};

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

/// Keeps us joined to a relay until a stop is requested.
pub fn run_relay(ctx: Arc<DaemonContext>) {
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
                Ok(listener) => serve_relay(&ctx, listener),
                Err(e) => log(&format!("Cannot join relay {addr}: {e}")),
            }
            if !ctx.sleep(RELAY_RETRY) {
                return;
            }
        }
    }
}

/// Waits for invitations on a joined relay until it is lost or a stop is requested.
fn serve_relay(ctx: &Arc<DaemonContext>, mut listener: RelayListener) {
    log(&format!(
        "Reachable through relay {}",
        listener.relay_addr()
    ));
    ctx.table().set_relay_uris(vec![listener.relay_uri()]);
    while !ctx.is_stopping() {
        match listener.poll_invitation(RELAY_POLL) {
            Ok(None) => {}
            Ok(Some(invitation)) => {
                let ctx = Arc::clone(ctx);
                let relay_addr = listener.relay_addr();
                let _ = std::thread::Builder::new()
                    .name("relay-session".to_owned())
                    .spawn(move || handle_invitation(&ctx, relay_addr, invitation));
            }
            Err(e) => {
                log(&format!("Lost relay {}: {e}", listener.relay_addr()));
                break;
            }
        }
    }
    ctx.table().set_relay_uris(Vec::new());
}

/// Joins the session a configured device requested through the relay.
fn handle_invitation(ctx: &DaemonContext, relay_addr: SocketAddr, invitation: SessionInvitation) {
    let Ok(raw_id) = <[u8; 32]>::try_from(invitation.from.as_slice()) else {
        return;
    };
    let from = DeviceId(raw_id);
    let Some(peer) = ctx.peer(&from) else {
        ctx.report_unknown_device("relay", from, || {
            format!(
                "Relay: unknown device {from} wants to connect. To accept it: syncthing-cli device add {from}"
            )
        });
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
