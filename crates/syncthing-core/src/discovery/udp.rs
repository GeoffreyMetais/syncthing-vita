//! Syncthing Local Discovery v4 UDP broadcast / listen cycle.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use super::announce::{Announce, DISCOVERY_PORT};
use super::local_ip::detect_local_ipv4;

/// Result of a Syncthing Local Discovery v4 broadcast and listen cycle.
#[derive(Debug)]
pub struct DiscoveryCycleResult {
    pub local_port: u16,
    pub bytes_sent: usize,
    pub discovered_peers: Vec<(SocketAddr, Announce)>,
}

/// Executes a full Syncthing Local Discovery v4 announcement and listening cycle
/// (global + directed `/24` subnet broadcast). When UDP `21027` is blocked or unanswered,
/// callers fall back to [`super::lan_scan::scan_lan_bep_peers`].
pub fn run_discovery_cycle(
    own_announce: &Announce,
    target_addr: &str,
    listen_duration: Duration,
) -> io::Result<DiscoveryCycleResult> {
    let socket =
        UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).or_else(|_| UdpSocket::bind("0.0.0.0:0"))?;

    let local_port = socket.local_addr()?.port();
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(180)))?;

    let packet = own_announce.encode_packet();
    let target: SocketAddr = target_addr
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let bytes_sent = socket.send_to(&packet, target).unwrap_or(0);

    // Also send to the directed `/24` subnet broadcast address (e.g. `192.168.1.255:21027`)
    if let Some(local_ip) = detect_local_ipv4() {
        let oct = local_ip.octets();
        let subnet_bcast = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(oct[0], oct[1], oct[2], 255)),
            target.port(),
        );
        if subnet_bcast != target {
            let _ = socket.send_to(&packet, subnet_bcast);
        }
    }

    let udp_window = listen_duration.min(Duration::from_millis(900));
    let start = Instant::now();
    let mut discovered_peers = Vec::new();
    let mut buf = [0u8; 1500];

    while start.elapsed() < udp_window {
        match socket.recv_from(&mut buf) {
            Ok((len, peer_addr)) => {
                if let Ok(announce) = Announce::decode_packet(&buf[..len]) {
                    if announce.id != own_announce.id
                        && !discovered_peers
                            .iter()
                            .any(|(_, a): &(SocketAddr, Announce)| a.id == announce.id)
                    {
                        discovered_peers.push((peer_addr, announce));
                    }
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        }
    }

    Ok(DiscoveryCycleResult {
        local_port,
        bytes_sent,
        discovered_peers,
    })
}
