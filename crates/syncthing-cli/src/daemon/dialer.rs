//! Keeps a session with one configured device: dials its known addresses (configured, LAN,
//! Global Discovery), then its relays, and retries.

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use syncthing_core::discovery::PeerEndpoint;
use syncthing_core::{connect_via_relay, lookup_peer_on, HttpsServer};

use super::context::{DaemonContext, KnownPeer};
use super::log::log;
use super::sessions::{run_session, Link, SessionEnd};
use super::timing::{
    CONNECTED_POLL, CONNECT_TIMEOUT, GLOBAL_LOOKUP_INTERVAL, HTTPS_TIMEOUT, NO_ADDRESS_POLL,
    RETRY_AFTER_ERROR, RETRY_AFTER_SESSION, RETRY_AWAITING_PAIRING, RETRY_JITTER,
};

/// Resolves a configured address (`tcp://host:port` or `host:port`); `dynamic` gives nothing.
pub fn resolve_configured(address: &str) -> Vec<SocketAddr> {
    let trimmed = address.trim();
    let host_port = trimmed.strip_prefix("tcp://").unwrap_or(trimmed);
    let host_port = host_port.split('/').next().unwrap_or("");
    if host_port.is_empty() || host_port.eq_ignore_ascii_case("dynamic") {
        return Vec::new();
    }
    host_port
        .to_socket_addrs()
        .map(|it| it.filter(|a| !a.ip().is_unspecified()).collect())
        .unwrap_or_default()
}

fn jitter() -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    RETRY_JITTER.mul_f64(f64::from(nanos % 1000) / 1000.0)
}

/// Candidate endpoints: direct ones first (configured, LAN, global), relays last.
fn targets(ctx: &DaemonContext, peer: &KnownPeer) -> Vec<PeerEndpoint> {
    let mut direct: Vec<PeerEndpoint> = Vec::new();
    let mut relays: Vec<PeerEndpoint> = Vec::new();
    let mut push = |ep: PeerEndpoint| {
        let list = match ep {
            PeerEndpoint::Direct(_) => &mut direct,
            PeerEndpoint::Relay { .. } => &mut relays,
        };
        if !list.contains(&ep) {
            list.push(ep);
        }
    };
    for address in &peer.addresses {
        resolve_configured(address)
            .into_iter()
            .for_each(|a| push(PeerEndpoint::Direct(a)));
    }
    let table = ctx.table();
    table
        .lan_addrs(&peer.id)
        .into_iter()
        .for_each(|a| push(PeerEndpoint::Direct(a)));
    table
        .global_endpoints(&peer.id)
        .into_iter()
        .for_each(&mut push);
    drop(table);
    if ctx.options.relays {
        direct.extend(relays);
    }
    direct
}

/// Looks the device up on the Global Discovery servers.
fn global_lookup(ctx: &DaemonContext, peer: &KnownPeer) {
    let mut endpoints = Vec::new();
    for url in &ctx.options.lookup_servers {
        let Some(server) = HttpsServer::parse(url) else {
            continue;
        };
        match lookup_peer_on(&server, &ctx.options.https, peer.id, HTTPS_TIMEOUT) {
            Ok(found) => endpoints.extend(found),
            Err(e) => {
                if ctx.first_report(format!("lookup:{}:{e}", peer.id)) {
                    log(&format!(
                        "Global Discovery lookup of {} on {}: {e}",
                        peer.name, server.host
                    ));
                }
            }
        }
    }
    ctx.table().set_global_endpoints(peer.id, endpoints);
}

fn label(ep: &PeerEndpoint) -> String {
    match ep {
        PeerEndpoint::Direct(addr) => format!("tcp://{addr}"),
        PeerEndpoint::Relay { addr, .. } => format!("relay://{addr}"),
    }
}

/// Dials `peer` until a stop is requested.
pub fn run_dialer(ctx: Arc<DaemonContext>, peer: KnownPeer) {
    let mut last_lookup: Option<Instant> = None;
    let mut last_error = String::new();
    let mut reported_no_address = false;
    while !ctx.is_stopping() {
        if ctx.table().is_claimed(&peer.id) {
            ctx.sleep(CONNECTED_POLL);
            continue;
        }
        if ctx.options.global_discovery
            && last_lookup.is_none_or(|t| t.elapsed() >= GLOBAL_LOOKUP_INTERVAL)
        {
            last_lookup = Some(Instant::now());
            global_lookup(&ctx, &peer);
        }
        let endpoints = targets(&ctx, &peer);
        if endpoints.is_empty() {
            if !reported_no_address {
                reported_no_address = true;
                log(&format!(
                    "No address known for {} yet, waiting for discovery",
                    peer.name
                ));
            }
            ctx.sleep(NO_ADDRESS_POLL);
            continue;
        }
        reported_no_address = false;

        let mut delay = RETRY_AFTER_ERROR;
        for ep in endpoints {
            if ctx.is_stopping() {
                return;
            }
            let link = match &ep {
                PeerEndpoint::Direct(addr) => Link::Dial(*addr),
                PeerEndpoint::Relay { addr, device_id } => {
                    match connect_via_relay(
                        &ctx.identity,
                        *addr,
                        *device_id,
                        peer.id,
                        CONNECT_TIMEOUT,
                    ) {
                        Ok(tcp) => Link::RelayClient { tcp, addr: *addr },
                        Err(e) => {
                            report_error(&peer, &ep, &e.to_string(), &mut last_error);
                            continue;
                        }
                    }
                }
            };
            match run_session(&ctx, &peer, link) {
                SessionEnd::Ran => {
                    last_error.clear();
                    delay = RETRY_AFTER_SESSION;
                    break;
                }
                SessionEnd::AlreadyConnected => {
                    delay = CONNECTED_POLL;
                    break;
                }
                SessionEnd::NotAccepted(reason) => {
                    report_error(&peer, &ep, &reason, &mut last_error);
                    delay = RETRY_AWAITING_PAIRING;
                    break;
                }
                SessionEnd::Failed(e) => report_error(&peer, &ep, &e.to_string(), &mut last_error),
            }
        }
        if !ctx.sleep(delay + jitter()) {
            break;
        }
    }
}

/// Logs a connection error, once until it changes.
fn report_error(peer: &KnownPeer, ep: &PeerEndpoint, error: &str, last_error: &mut String) {
    let message = format!("Cannot sync with {} at {}: {error}", peer.name, label(ep));
    if message != *last_error {
        log(&message);
        *last_error = message;
    }
}
