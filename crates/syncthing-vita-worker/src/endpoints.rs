//! Where to reach a peer, cheapest first: its configured address, then the addresses learned
//! from the cycle's LAN discovery round, then Global Discovery (direct addresses before
//! relays). Mirrors the app's `peer_endpoints::collect_peer_targets`.

use std::net::{SocketAddr, ToSocketAddrs};

use syncthing_core::{resolve_peer_tcp_addrs, DeviceId, DiscoveryCycleResult, PeerEndpoint};

use crate::config::DeviceRecord;

/// Addresses learned during the cycle, by device.
#[derive(Debug, Clone, Default)]
pub struct KnownEndpoints {
    lan: Vec<(DeviceId, Vec<SocketAddr>)>,
    global: Vec<(DeviceId, Vec<PeerEndpoint>)>,
}

impl KnownEndpoints {
    /// Records the announcements of a LAN discovery round (a device's latest one wins).
    pub fn add_lan(&mut self, result: &DiscoveryCycleResult) {
        for (udp_src, announce) in &result.discovered_peers {
            let addrs = resolve_peer_tcp_addrs(*udp_src, &announce.addresses);
            self.lan.retain(|(id, _)| *id != announce.id);
            self.lan.push((announce.id, addrs));
        }
    }

    /// Records what Global Discovery returned for `device`.
    pub fn add_global(&mut self, device: DeviceId, endpoints: Vec<PeerEndpoint>) {
        self.global.retain(|(id, _)| *id != device);
        self.global.push((device, endpoints));
    }

    /// Candidate endpoints of `peer` in the order to try them, with the full device ID to
    /// expect (a short ID placeholder is replaced by the ID a discovery source announced).
    pub fn targets(&self, peer: &DeviceRecord) -> (DeviceId, Vec<PeerEndpoint>) {
        let mut device_id = peer.device_id;
        let mut direct = Vec::new();
        let mut relays = Vec::new();
        for addr in resolve_configured_addr(&peer.address) {
            push_unique(&mut direct, PeerEndpoint::Direct(addr));
        }
        for (id, addrs) in &self.lan {
            if peer.device_id.matches_peer(id) {
                device_id = *id;
                for addr in addrs {
                    push_unique(&mut direct, PeerEndpoint::Direct(*addr));
                }
            }
        }
        for (id, endpoints) in &self.global {
            if peer.device_id.matches_peer(id) {
                device_id = *id;
                for ep in endpoints {
                    match ep {
                        PeerEndpoint::Direct(_) => push_unique(&mut direct, ep.clone()),
                        PeerEndpoint::Relay { .. } => push_unique(&mut relays, ep.clone()),
                    }
                }
            }
        }
        direct.extend(relays);
        (device_id, direct)
    }
}

/// Resolves a configured address (`ip:port` or `hostname:port`). `dynamic`, empty,
/// unspecified or unresolvable addresses give nothing.
pub fn resolve_configured_addr(addr: &str) -> Vec<SocketAddr> {
    let trimmed = addr.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("dynamic") {
        return Vec::new();
    }
    if let Ok(sock) = trimmed.parse::<SocketAddr>() {
        return (!sock.ip().is_unspecified())
            .then_some(sock)
            .into_iter()
            .collect();
    }
    trimmed
        .to_socket_addrs()
        .map(|iter| iter.filter(|s| !s.ip().is_unspecified()).collect())
        .unwrap_or_default()
}

fn push_unique(out: &mut Vec<PeerEndpoint>, ep: PeerEndpoint) {
    if !out.contains(&ep) {
        out.push(ep);
    }
}

/// Label of an endpoint in the log.
pub fn endpoint_label(ep: &PeerEndpoint) -> String {
    match ep {
        PeerEndpoint::Direct(addr) => addr.to_string(),
        PeerEndpoint::Relay { addr, .. } => format!("relay://{addr}"),
    }
}
