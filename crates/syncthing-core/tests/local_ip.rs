//! Local IPv4 detection (`discovery::local_ip`): loopback and unspecified addresses are never
//! reported, and the primary address is found when the host has a route.

use std::net::{SocketAddr, UdpSocket};

use syncthing_core::discovery::local_ip::{detect_local_ipv4, detect_local_ipv4_from};

#[test]
fn a_loopback_local_address_is_not_reported() {
    // A UDP "connect" to a loopback target makes the local address 127.0.0.1: not a LAN address.
    assert_eq!(detect_local_ipv4_from(&["127.0.0.1:80"]), None);
    assert_eq!(detect_local_ipv4_from(&[]), None);
}

#[test]
fn the_primary_address_is_found_when_the_host_has_a_route() {
    // Independent check, no packet is sent: does this host have a route to a public address?
    let routed = UdpSocket::bind("0.0.0.0:0")
        .and_then(|sock| {
            sock.connect("8.8.8.8:80")?;
            sock.local_addr()
        })
        .ok();
    let Some(SocketAddr::V4(local)) = routed else {
        eprintln!("no route to a public address: primary address test skipped");
        return;
    };
    if local.ip().is_loopback() || local.ip().is_unspecified() {
        return;
    }
    // Which interface wins depends on the host's routes: only the kind of address is checked.
    let detected = detect_local_ipv4().expect("a local address with a route");
    assert!(
        !detected.is_loopback() && !detected.is_unspecified(),
        "{detected}"
    );
}
