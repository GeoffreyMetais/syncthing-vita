//! `syncthing-core`: OS-agnostic Syncthing protocol implementation (BEP/1.0, TLS 1.3 & Local Discovery v4).
//! Tested natively on `x86_64-unknown-linux-gnu` via `cargo test -p syncthing-core`.

pub mod bep;
pub mod disco;
pub mod proto;
pub mod storage;
pub mod tls;

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use bep::{
    decompress_bep_lz4, read_bep_message, write_bep_message, BepBlockInfo, BepClose,
    BepClusterConfig, BepCompression, BepFileInfo, BepFolder, BepHeader, BepHello, BepIndex,
    BepMessageType, BepRequest, BepResponse, BepVersionCounter, FolderIndexState, BEP_BLOCK_SIZE,
    BEP_DEFAULT_PORT, BEP_MAGIC,
};
pub use disco::{
    Announce, DeviceId, DISCOVERY_BROADCAST_ADDR, DISCOVERY_MAGIC, DISCOVERY_PORT,
};
pub use storage::FsSyncStorage;
pub use tls::{
    connect_and_sync, probe_bep_peer_device_id, PeerHandshakeResult, SyncEvent, SyncSessionStatus,
    VitaIdentity, BEP_ALPN,
};

/// Result of a Syncthing Local Discovery v4 broadcast and listen cycle.
#[derive(Debug)]
pub struct DiscoveryCycleResult {
    pub local_port: u16,
    pub bytes_sent: usize,
    pub discovered_peers: Vec<(SocketAddr, Announce)>,
}

/// Detects the primary local IPv4 address via OS routing table lookup without transmitting packets.
pub fn detect_local_ipv4() -> Option<Ipv4Addr> {
    for probe_target in ["192.168.1.1:80", "8.8.8.8:80", "10.0.0.1:80"] {
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

/// Scans the local `/24` LAN subnet (plus `127.0.0.1`) on TCP port `22000` (`BEP_DEFAULT_PORT`)
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

    let mut open_list = open_addrs
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
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

/// Executes a full Syncthing Local Discovery v4 announcement and listening cycle,
/// automatically falling back to a fast LAN subnet TCP `22000` (`bep/1.0` TLS 1.3) scan
/// when UDP port `21027` broadcasts are blocked or unanswered on the network.
pub fn run_discovery_cycle(
    own_announce: &Announce,
    target_addr: &str,
    listen_duration: Duration,
) -> io::Result<DiscoveryCycleResult> {
    let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))
        .or_else(|_| UdpSocket::bind("0.0.0.0:0"))?;

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
                        && !discovered_peers.iter().any(|(_, a): &(SocketAddr, Announce)| a.id == announce.id)
                    {
                        discovered_peers.push((peer_addr, announce));
                    }
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        }
    }

    // If UDP broadcast on port 21027 found no peers (e.g. Docker container / firewall / AP isolation),
    // automatically scan the local /24 subnet on TCP port 22000 (BEP/1.0 TLS 1.3)
    if discovered_peers.is_empty() && target.port() == DISCOVERY_PORT {
        discovered_peers = scan_lan_bep_peers(own_announce.id);
    }

    Ok(DiscoveryCycleResult {
        local_port,
        bytes_sent,
        discovered_peers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_announce_protobuf_roundtrip() {
        let id = DeviceId([0x42; 32]);
        let original = Announce {
            id,
            addresses: vec![
                "tcp://192.168.1.55:22000".to_owned(),
                "quic://192.168.1.55:22000".to_owned(),
            ],
            instance_id: 0x1234_5678_9ABC_DEF0,
        };

        let encoded = original.encode_packet();
        assert_eq!(&encoded[0..4], &[0x2E, 0xA7, 0xD9, 0x0B]);

        let decoded = Announce::decode_packet(&encoded).expect("Announce decoding failed");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_device_id_formatting() {
        let id = DeviceId([0u8; 32]);
        let formatted = id.to_syncthing_string();
        assert_eq!(formatted.len(), 63);
        assert_eq!(formatted.matches('-').count(), 7);
        assert!(formatted.starts_with("AAAAAAA-AAAAAA"));
    }

    #[test]
    fn test_device_id_parse_full_and_short() {
        let seed = *b"VITA_SYNCTHING_ECDSA_SEED_000001";
        let full_id = VitaIdentity::from_seed(&seed).unwrap().device_id;
        let full_str = full_id.to_syncthing_string();
        let short_str = full_id.short();

        // 1. Full 56-char Syncthing Device ID roundtrip (with hyphens and lowercase)
        let parsed_full = DeviceId::parse_user_id(&full_str.to_lowercase()).unwrap();
        assert_eq!(parsed_full, full_id);
        assert!(!parsed_full.is_short_placeholder());

        // 2. Typo in full 56-char Device ID must be rejected by Luhn mod 32 check
        let mut corrupted = full_str.clone();
        corrupted.replace_range(0..1, if full_str.starts_with('A') { "B" } else { "A" });
        assert!(DeviceId::parse_user_id(&corrupted).is_err());

        // 3. Short 7-char Device ID parsing & peer matching
        let parsed_short = DeviceId::parse_user_id(&short_str).unwrap();
        assert!(parsed_short.is_short_placeholder());
        assert_eq!(parsed_short.short(), short_str);
        assert_eq!(parsed_short.syncthing_prefix(), short_str);
        assert!(parsed_short.matches_peer(&full_id));
        assert!(full_id.matches_peer(&parsed_short));

        // 4. Persistence roundtrip via hex preserves short placeholder & prefix matching
        let reloaded = DeviceId::from_hex(&parsed_short.to_hex()).unwrap();
        assert!(reloaded.is_short_placeholder());
        assert!(reloaded.matches_peer(&full_id));

        // 5. Two-group (14-char) prefix with Luhn check digit
        let two_groups = &full_str[..15]; // "XXXXXXX-XXXXXXX"
        let parsed_14 = DeviceId::parse_user_id(two_groups).unwrap();
        assert!(parsed_14.is_short_placeholder());
        assert!(parsed_14.matches_peer(&full_id));
    }

    #[test]
    fn test_bep_hello_roundtrip() {
        let hello = BepHello::new_vita_default();
        let frame = hello.encode_frame();
        assert_eq!(&frame[0..4], &[0x2E, 0xA7, 0xD9, 0x0B]);

        let decoded = BepHello::decode_frame(&frame).expect("BEP Hello decoding failed");
        assert_eq!(decoded, hello);
    }

    #[test]
    fn test_vita_x509_identity_generation() {
        let seed = *b"VITA_SYNCTHING_ECDSA_SEED_000001";
        let id1 = VitaIdentity::from_seed(&seed).expect("X.509 identity generation failed");
        let id2 = VitaIdentity::from_seed(&seed).expect("X.509 identity generation failed");
        assert_eq!(id1.device_id, id2.device_id);
        assert!(!id1.cert_der.is_empty());
        assert!(!id1.key_pkcs8_der.is_empty());
    }

    #[test]
    fn test_lz4_decompression() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&12u32.to_be_bytes());
        payload.push(0x80);
        payload.extend_from_slice(b"VITA_BEP");
        payload.extend_from_slice(&8u16.to_le_bytes());
        let decompressed = decompress_bep_lz4(&payload).expect("LZ4 decompression failed");
        assert_eq!(&decompressed, b"VITA_BEPVITA");
    }

    #[test]
    fn test_delta_sync_and_local_index_roundtrip() {
        let tmp_dir = std::env::temp_dir().join("vita_syncthing_delta_test");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let storage = FsSyncStorage::new(&tmp_dir);
        let dev_id = DeviceId([0x11; 32]);

        // Streaming block write
        let sample_data = b"Hello from PS Vita streaming block storage!";
        storage
            .write_block_streaming("test-folder", "save01.bin", 0, sample_data, sample_data.len() as u64)
            .expect("Streaming write failed");

        // Local indexing
        let (local_idx, state) = storage
            .build_local_index("test-folder", dev_id, &[])
            .expect("Local indexing failed");
        assert_eq!(local_idx.files.len(), 1);
        assert_eq!(state.max_sequence, 1);

        // Protobuf roundtrip of BepIndex
        let encoded_idx = local_idx.encode();
        let decoded_idx = BepIndex::decode(&encoded_idx).expect("BepIndex decoding failed");
        assert_eq!(decoded_idx.files.len(), 1);
        assert_eq!(decoded_idx.files[0].name, "save01.bin");

        // Delta sync check: file is already identical on disk -> 0 outdated blocks
        let outdated = storage
            .find_outdated_blocks("test-folder", &decoded_idx.files[0])
            .expect("Delta check failed");
        assert!(outdated.is_empty(), "No blocks should be re-downloaded");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
