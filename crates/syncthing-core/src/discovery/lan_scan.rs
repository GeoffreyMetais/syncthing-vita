//! Fallback LAN peer discovery: TCP `22000` subnet scan verified with a TLS `bep/1.0` probe.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::announce::Announce;
use super::local_ip::detect_local_ipv4;
use crate::bep::BEP_DEFAULT_PORT;
use crate::device_id::DeviceId;
use crate::tls::probe_bep_peer_device_id;

/// and verifies any open port using a TLS 1.3 (`bep/1.0`) handshake to extract its Syncthing `DeviceId`.
/// Uses bounded concurrency (16 lightweight workers) safe for PS Vita (`sceNet`) and Vita3K.
pub fn scan_lan_bep_peers(own_device_id: DeviceId) -> Vec<(SocketAddr, Announce)> {
    let local_ip = detect_local_ipv4().unwrap_or(Ipv4Addr::new(192, 168, 1, 0));
    let octets = local_ip.octets();
    let self_host = octets[3];

    let mut candidates = Vec::with_capacity(255);
    candidates.push(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        BEP_DEFAULT_PORT,
    ));
    for host in 1u8..=254u8 {
        if host == self_host {
            continue;
        }
        let ip = Ipv4Addr::new(octets[0], octets[1], octets[2], host);
        candidates.push(SocketAddr::new(IpAddr::V4(ip), BEP_DEFAULT_PORT));
    }

    let queue = Arc::new(Mutex::new(candidates.into_iter()));
    let open_addrs = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for w in 0..16 {
        let q = Arc::clone(&queue);
        let found = Arc::clone(&open_addrs);
        let builder = std::thread::Builder::new()
            .name(format!("lan_scan_{w}"))
            .stack_size(256 * 1024);
        if let Ok(h) = builder.spawn(move || loop {
            let next_addr = {
                let Ok(mut guard) = q.lock() else {
                    break;
                };
                guard.next()
            };
            let Some(addr) = next_addr else {
                break;
            };
            if TcpStream::connect_timeout(&addr, Duration::from_millis(75)).is_ok() {
                if let Ok(mut out) = found.lock() {
                    out.push(addr);
                }
            }
        }) {
            handles.push(h);
        }
    }

    for h in handles {
        let _ = h.join();
    }

    let mut open_list = open_addrs.lock().map(|g| g.clone()).unwrap_or_default();
    open_list.sort();

    let mut peers = Vec::new();
    for addr in open_list {
        if let Some(peer_id) = probe_bep_peer_device_id(addr, Duration::from_millis(1200)) {
            if peer_id != own_device_id {
                peers.push((
                    addr,
                    Announce {
                        id: peer_id,
                        addresses: vec![format!("tcp://{}", addr)],
                        instance_id: 0,
                    },
                ));
            }
        }
    }

    peers
}
