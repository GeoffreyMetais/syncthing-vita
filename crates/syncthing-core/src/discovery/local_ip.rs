//! Local IPv4 address detection (routing-table lookup, no packets sent).

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

/// Detects the primary local IPv4 address via OS routing table lookup without transmitting packets.
pub fn detect_local_ipv4() -> Option<Ipv4Addr> {
    detect_local_ipv4_from(&["192.168.1.1:80", "8.8.8.8:80", "10.0.0.1:80"])
}

/// [`detect_local_ipv4`] through the given `host:port` probe targets, tried in order. A
/// loopback or unspecified local address is never returned.
pub fn detect_local_ipv4_from(probe_targets: &[&str]) -> Option<Ipv4Addr> {
    for probe_target in probe_targets {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(probe_target).is_ok() {
                if let Ok(SocketAddr::V4(local_v4)) = sock.local_addr() {
                    let ip = *local_v4.ip();
                    if !ip.is_unspecified() && !ip.is_loopback() {
                        return Some(ip);
                    }
                }
            }
        }
    }
    None
}
