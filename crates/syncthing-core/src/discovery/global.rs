//! Syncthing Global Discovery v2 lookup (`https://discovery-lookup.syncthing.net/v2/`);
//! announcements are in [`super::global_announce`].
//!
//! Queries a peer's announced `tcp://` and `relay://` endpoints over TLS so devices outside the
//! local LAN can still discover and connect to each other.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

use super::http::{https_request, json_string_array};
use crate::device_id::DeviceId;
use crate::tls::https::HttpsServer;

/// Public Syncthing Global Discovery v2 lookup server. Like Syncthing's default, the URL pins
/// the certificate of the official discovery servers (`?id=`); a certificate chaining to a
/// public root is accepted as well (see [`crate::tls::https`]).
pub const DEFAULT_LOOKUP_SERVER: &str = concat!(
    "https://discovery-lookup.syncthing.net/v2/?noannounce&id=",
    "LYXKCHX-VI3NYZR-ALCJBHF-WMZYSPK-QG6QJA3-MPFYMSO-U56GTUK-NA2MIAW"
);

/// A WAN endpoint returned by Global Discovery v2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerEndpoint {
    /// Direct TCP endpoint (e.g. `tcp://203.0.113.10:22000`).
    Direct(SocketAddr),
    /// Syncthing Relay v1 endpoint (e.g. `relay://198.51.100.5:22067/?id=...`).
    Relay {
        addr: SocketAddr,
        device_id: DeviceId,
    },
}

/// Resolves `host_port` (either `ip:port` or `hostname:port`) to a `SocketAddr`.
fn resolve_socket_addr(host_port: &str) -> Option<SocketAddr> {
    if let Ok(sock) = host_port.parse::<SocketAddr>() {
        return (!sock.ip().is_unspecified()).then_some(sock);
    }
    host_port
        .to_socket_addrs()
        .ok()?
        .find(|s| !s.ip().is_unspecified())
}

/// Turns a single Syncthing URI (`tcp://...` or `relay://...`) into a [`PeerEndpoint`].
/// A host name is resolved through DNS (this is not a pure parser: it may block on the
/// network); `None` for unsupported schemes, unresolvable hosts and unspecified addresses.
pub fn resolve_endpoint_uri(uri: &str) -> Option<PeerEndpoint> {
    if let Some(rest) = uri.strip_prefix("tcp://") {
        let host_port = rest.split(['/', '?']).next()?;
        let addr = resolve_socket_addr(host_port)?;
        return Some(PeerEndpoint::Direct(addr));
    }
    if let Some(rest) = uri.strip_prefix("relay://") {
        let (authority, query) = match rest.split_once('?') {
            Some((left, right)) => (left.trim_end_matches('/'), right),
            None => return None,
        };
        let addr = resolve_socket_addr(authority)?;
        let id_str = query.split('&').find_map(|kv| kv.strip_prefix("id="))?;
        let device_id = DeviceId::parse_user_id(id_str).ok()?;
        return Some(PeerEndpoint::Relay { addr, device_id });
    }
    None
}

/// Extracts the string elements of `"addresses": [...]` from a Global Discovery v2 JSON body.
pub fn parse_global_discovery_json(json: &str) -> Vec<String> {
    json_string_array(json, "addresses")
}

/// Queries the default Global Discovery server ([`DEFAULT_LOOKUP_SERVER`]) for `device_id`.
pub fn lookup_global_peer(device_id: DeviceId, timeout: Duration) -> io::Result<Vec<PeerEndpoint>> {
    let server = HttpsServer::parse(DEFAULT_LOOKUP_SERVER)
        .ok_or_else(|| io::Error::other("Invalid default lookup server"))?;
    lookup_peer_on(&server, device_id, timeout)
}

/// Queries the Global Discovery v2 `server` for `device_id` and returns its announced direct
/// and relay endpoints (empty if the device is unknown to the server).
pub fn lookup_peer_on(
    server: &HttpsServer,
    device_id: DeviceId,
    timeout: Duration,
) -> io::Result<Vec<PeerEndpoint>> {
    let base = server.path.split('?').next().unwrap_or("/");
    let path = format!("{base}?device={}", device_id.to_syncthing_string());
    let response = https_request(server, "GET", &path, None, None, timeout)?;
    if response.status == 404 {
        return Ok(Vec::new());
    }
    if response.status != 200 {
        return Err(io::Error::other(format!(
            "Global Discovery HTTP {}",
            response.status
        )));
    }

    let mut endpoints = Vec::new();
    for uri in parse_global_discovery_json(&response.body) {
        if let Some(ep) = resolve_endpoint_uri(&uri) {
            if !endpoints.contains(&ep) {
                endpoints.push(ep);
            }
        }
    }
    Ok(endpoints)
}
