//! TCP `22000` LAN fallback scan (`discovery::lan_scan::scan_lan_bep_peers`). The scan always
//! probes `127.0.0.1:22000` first, so the test runs a BEP peer on that port. When the port is
//! busy on this machine, the test is skipped (it cannot pick another port).

mod common;

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::tls_server::{spawn_tls_server_on, test_identity};
use syncthing_core::discovery::lan_scan::{
    scan_lan_bep_peers, scan_lan_bep_peers_interruptible, scan_lan_bep_peers_with_options,
};
use syncthing_core::{Announce, DeviceId, BEP_ALPN};

const BEP_PORT: u16 = 22000;
const PEER_SEED: u8 = 0x91;
const OTHER_SEED: u8 = 0x92;

/// Every test in this binary probes `127.0.0.1:22000`, so they must not run concurrently.
static SCAN_LOCK: Mutex<()> = Mutex::new(());

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
    let _guard = SCAN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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

#[test]
fn interruptible_scan_aborts_immediately_when_stop_is_already_requested() {
    let _guard = SCAN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let own = test_identity(OTHER_SEED).device_id;
    let start = Instant::now();
    let found = scan_lan_bep_peers_interruptible(own, || true);
    assert!(found.is_empty());
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "already-stopped scan took {:?}",
        start.elapsed()
    );
}

#[test]
fn interruptible_scan_stops_early_when_signalled_mid_scan() {
    let _guard = SCAN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let own = test_identity(OTHER_SEED).device_id;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_bg = Arc::clone(&stop);
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        stop_bg.store(true, Ordering::Release);
    });

    let start = Instant::now();
    let _found = scan_lan_bep_peers_interruptible(own, || stop.load(Ordering::Acquire));
    let elapsed = start.elapsed();
    trigger.join().unwrap();

    assert!(
        elapsed < Duration::from_secs(1),
        "mid-scan cancellation took too long: {elapsed:?}"
    );
}

#[test]
fn the_scan_skips_loopback_when_excluded() {
    let _guard = SCAN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Ok(listener) = TcpListener::bind(("127.0.0.1", BEP_PORT)) else {
        eprintln!("port {BEP_PORT} is busy: loopback-exclusion LAN scan test skipped");
        return;
    };
    listener
        .set_nonblocking(true)
        .expect("non-blocking listener");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_bg = Arc::clone(&stop);
    let accepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let accepted_bg = Arc::clone(&accepted);
    let acceptor = std::thread::spawn(move || {
        while !stop_bg.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    accepted_bg.fetch_add(1, Ordering::Relaxed);
                    drop(stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });

    let own = test_identity(OTHER_SEED).device_id;
    let found = scan_lan_bep_peers_with_options(own, false, || false);
    stop.store(true, Ordering::Release);
    acceptor.join().unwrap();

    assert_eq!(
        accepted.load(Ordering::Relaxed),
        0,
        "loopback port {BEP_PORT} should not be probed when include_loopback is false"
    );
    assert!(
        found.iter().all(|(addr, _)| !addr.ip().is_loopback()),
        "loopback peer unexpectedly reported: {found:?}"
    );
}
