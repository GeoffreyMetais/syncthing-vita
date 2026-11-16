//! TCP `22000` LAN fallback scan (`discovery::lan_scan::scan_lan_bep_peers`). The scan always
//! probes `127.0.0.1:22000` first, so the test runs a BEP peer on that port. When the port is
//! busy on this machine, the test is skipped (it cannot pick another port).

mod common;

use std::io::Read;
use std::net::{SocketAddr, TcpListener};

use common::tls_server::{spawn_tls_server_on, test_identity};
use syncthing_core::discovery::lan_scan::scan_lan_bep_peers;
use syncthing_core::{Announce, DeviceId, BEP_ALPN};

const BEP_PORT: u16 = 22000;
const PEER_SEED: u8 = 0x91;
const OTHER_SEED: u8 = 0x92;

/// Runs one scan with a BEP peer listening on `127.0.0.1:22000`. `None` when the port is busy.
fn scan_with_peer(own: DeviceId) -> Option<(Vec<(SocketAddr, Announce)>, DeviceId)> {
    let listener = TcpListener::bind(("127.0.0.1", BEP_PORT)).ok()?;
    let peer = test_identity(PEER_SEED).device_id;
    // The scan first checks the port with a plain TCP connection (dropped at once), then
    // probes it with TLS: the second connection is the one that answers.
    let server = spawn_tls_server_on(
        listener,
        1,
        test_identity(PEER_SEED),
        BEP_ALPN,
        false,
        |stream| {
            let mut byte = [0u8; 1];
            let _ = stream.read(&mut byte);
        },
    );
    let found = scan_lan_bep_peers(own);
    server.join();
    Some((found, peer))
}

#[test]
fn the_scan_finds_a_bep_peer_on_the_loopback_port_and_skips_our_own_id() {
    let own = test_identity(OTHER_SEED).device_id;
    let Some((found, peer)) = scan_with_peer(own) else {
        eprintln!("port {BEP_PORT} is busy: LAN scan test skipped");
        return;
    };
    let expected = Announce {
        id: peer,
        addresses: vec![format!("tcp://127.0.0.1:{BEP_PORT}")],
        instance_id: 0,
    };
    let loopback: SocketAddr = format!("127.0.0.1:{BEP_PORT}").parse().expect("address");
    assert!(
        found.contains(&(loopback, expected)),
        "peer not found: {found:?}"
    );

    // Our own device, when it answers on the port, is not a peer.
    let Some((found, peer)) = scan_with_peer(peer) else {
        return;
    };
    assert!(
        found.iter().all(|(_, announce)| announce.id != peer),
        "own device listed as a peer: {found:?}"
    );
}
