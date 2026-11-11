//! Resolution of the TCP addresses advertised in an `Announce` packet.

use std::net::SocketAddr;

/// Extracts and orders reachable TCP `SocketAddr` addresses from an `Announce` packet
/// and the UDP datagram source IP address.
pub fn resolve_peer_tcp_addrs(udp_src: SocketAddr, addresses: &[String]) -> Vec<SocketAddr> {
    let mut exact_ip_matches = Vec::new();
    let mut other_matches = Vec::new();

    for addr_str in addresses {
        if let Some(rest) = addr_str.strip_prefix("tcp://") {
            let host_port = rest.split('/').next().unwrap_or(rest);
            if let Ok(mut sock_addr) = host_port.parse::<SocketAddr>() {
                if sock_addr.ip().is_unspecified() {
                    sock_addr.set_ip(udp_src.ip());
                }
                if sock_addr.ip() == udp_src.ip() {
                    if !exact_ip_matches.contains(&sock_addr) {
                        exact_ip_matches.push(sock_addr);
                    }
                } else if !other_matches.contains(&sock_addr) {
                    other_matches.push(sock_addr);
                }
            }
        }
    }

    exact_ip_matches.extend(other_matches);
    exact_ip_matches
}
