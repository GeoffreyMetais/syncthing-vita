//! Local Discovery v4 cycle (`discovery::udp::run_discovery_cycle`) against loopback peers: the
//! announcement is sent to the given address, answers are collected until the listen window
//! ends, our own announcement is ignored, and an unparsable target is an error.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use syncthing_core::discovery::udp::run_discovery_cycle;
use syncthing_core::{Announce, DeviceId};

/// A loopback peer: receives one announcement, waits `delay`, then answers with `answer`.
/// Returns its address and the announcement it received.
fn spawn_peer(delay: Duration, answer: Announce) -> (SocketAddr, JoinHandle<Vec<u8>>) {
    let sock = UdpSocket::bind("127.0.0.1:0").expect("bind peer");
    sock.set_read_timeout(Some(Duration::from_secs(3)))
        .expect("read timeout");
    let addr = sock.local_addr().expect("peer address");
    let handle = thread::spawn(move || {
        let mut buf = [0u8; 1500];
        let (len, from) = sock.recv_from(&mut buf).expect("announcement");
        thread::sleep(delay);
        sock.send_to(&answer.encode_packet(), from).expect("answer");
        buf[..len].to_vec()
    });
    (addr, handle)
}

fn own_announce() -> Announce {
    Announce {
        id: DeviceId([0x61; 32]),
        addresses: vec!["tcp://127.0.0.1:22000".to_owned()],
        instance_id: 7,
    }
}

fn peer_announce(seed: u8) -> Announce {
    Announce {
        id: DeviceId([seed; 32]),
        addresses: vec![format!("tcp://127.0.0.1:{}", 22000 + u16::from(seed))],
        instance_id: i64::from(seed),
    }
}

#[test]
fn an_answer_within_the_window_is_discovered() {
    let peer = peer_announce(0x62);
    let (addr, received) = spawn_peer(Duration::from_millis(100), peer.clone());
    let own = own_announce();
    let result = run_discovery_cycle(&own, &addr.to_string(), Duration::from_millis(600))
        .expect("discovery cycle");

    let sent = received.join().expect("peer");
    assert_eq!(Announce::decode_packet(&sent).expect("announcement"), own);
    assert_eq!(result.bytes_sent, own.encode_packet().len());
    assert_eq!(
        result.discovered_peers.len(),
        1,
        "{:?}",
        result.discovered_peers
    );
    assert_eq!(result.discovered_peers[0].0, addr);
    assert_eq!(result.discovered_peers[0].1, peer);
}

#[test]
fn an_answer_after_a_read_timeout_is_still_discovered() {
    // The socket reads with a 180 ms timeout: the answer arrives after the first timeouts.
    let peer = peer_announce(0x63);
    let (addr, received) = spawn_peer(Duration::from_millis(400), peer.clone());
    let result = run_discovery_cycle(
        &own_announce(),
        &addr.to_string(),
        Duration::from_millis(1000),
    )
    .expect("discovery cycle");
    received.join().expect("peer");
    assert_eq!(
        result.discovered_peers.len(),
        1,
        "{:?}",
        result.discovered_peers
    );
    assert_eq!(result.discovered_peers[0].1, peer);
}

#[test]
fn our_own_announcement_echoed_back_is_not_a_peer() {
    let (addr, received) = spawn_peer(Duration::from_millis(50), own_announce());
    let result = run_discovery_cycle(
        &own_announce(),
        &addr.to_string(),
        Duration::from_millis(400),
    )
    .expect("discovery cycle");
    received.join().expect("peer");
    assert!(
        result.discovered_peers.is_empty(),
        "{:?}",
        result.discovered_peers
    );
}

#[test]
fn an_unparsable_target_is_an_invalid_input_error() {
    let err = run_discovery_cycle(&own_announce(), "not an address", Duration::from_millis(10))
        .expect_err("invalid target");
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
}
