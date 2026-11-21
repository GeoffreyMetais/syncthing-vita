//! Fallback LAN peer discovery: TCP `22000` subnet scan verified with a TLS `bep/1.0` probe.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::Duration;

use super::announce::Announce;
use super::local_ip::detect_local_ipv4;
use crate::bep::BEP_DEFAULT_PORT;
use crate::device_id::DeviceId;
use crate::tls::probe_bep_peer_device_id;

const SCAN_WORKERS: usize = if cfg!(target_os = "vita") { 8 } else { 16 };

/// Scans the local `/24` subnet on TCP port `22000` (plus `127.0.0.1:22000`) and verifies any open
/// port using a TLS 1.3 (`bep/1.0`) handshake to extract its Syncthing `DeviceId`.
/// Uses bounded concurrency safe for PS Vita (`sceNet`) and Vita3K.
pub fn scan_lan_bep_peers(own_device_id: DeviceId) -> Vec<(SocketAddr, Announce)> {
    scan_lan_bep_peers_interruptible(own_device_id, || false)
}

/// Interruptible variant of [`scan_lan_bep_peers`] that aborts pending TCP connect and TLS probe
/// steps as soon as `should_stop()` returns `true`.
pub fn scan_lan_bep_peers_interruptible<F>(
    own_device_id: DeviceId,
    should_stop: F,
) -> Vec<(SocketAddr, Announce)>
where
    F: Fn() -> bool + Send + Sync,
{
    if should_stop() {
        return Vec::new();
    }

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

    let queue = Mutex::new(candidates.into_iter());
    let open_addrs = Mutex::new(Vec::new());
    let stop = &should_stop;

    std::thread::scope(|scope| {
        for w in 0..SCAN_WORKERS {
            let q = &queue;
            let found = &open_addrs;
            let builder = std::thread::Builder::new()
                .name(format!("lan_scan_{w}"))
                .stack_size(256 * 1024);
            let _ = builder.spawn_scoped(scope, move || loop {
                if stop() {
                    break;
                }
                let next_addr = {
                    let Ok(mut guard) = q.lock() else {
                        break;
                    };
                    guard.next()
                };
                let Some(addr) = next_addr else {
                    break;
                };
                if stop() {
                    break;
                }
                if TcpStream::connect_timeout(&addr, Duration::from_millis(75)).is_ok() {
                    if let Ok(mut out) = found.lock() {
                        out.push(addr);
                    }
                }
            });
        }
    });

    if should_stop() {
        return Vec::new();
    }

    let mut open_list = open_addrs.into_inner().unwrap_or_default();
    open_list.sort();

    let mut peers = Vec::new();
    for addr in open_list {
        if should_stop() {
            break;
        }
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
