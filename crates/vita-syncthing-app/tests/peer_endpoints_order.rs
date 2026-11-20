//! Candidate endpoint selection per configured device: address spellings, short-ID matching of
//! discovered peers, duplicate removal and the direct-before-relay order.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use syncthing_core::{DeviceId, PeerEndpoint};
use vita_syncthing_app::domain::KnownDevice;
use vita_syncthing_app::infrastructure::peer_endpoints::{
    collect_peer_targets, needs_global_lookup, resolve_configured_addr, SharedDiscoveredAddrs,
    SharedGlobalEndpoints,
};

fn sock(text: &str) -> SocketAddr {
    text.parse().unwrap()
}

fn device(id: DeviceId, address: &str) -> KnownDevice {
    KnownDevice {
        device_id: id,
        name: "Peer".to_owned(),
        address: address.to_owned(),
    }
}

fn discovered(entries: Vec<(DeviceId, Vec<SocketAddr>)>) -> SharedDiscoveredAddrs {
    Arc::new(Mutex::new(entries))
}

fn global(entries: Vec<(DeviceId, Vec<PeerEndpoint>)>) -> SharedGlobalEndpoints {
    Arc::new(Mutex::new(entries))
}

const DYNAMIC_IS_EMPTY: &[&str] = &[
    "DYNAMIC",
    " Dynamic ",
    "",
    "   ",
    "localhost",
    "0.0.0.0:22000",
];

#[test]
fn test_configured_address_is_trimmed_and_ipv6_supported() {
    assert_eq!(
        resolve_configured_addr("  10.1.2.3:22000  "),
        vec![sock("10.1.2.3:22000")]
    );
    assert_eq!(
        resolve_configured_addr("[::1]:22000"),
        vec![sock("[::1]:22000")]
    );
    assert!(
        resolve_configured_addr("[::]:22000").is_empty(),
        "unspecified IPv6 is refused"
    );
    assert!(DYNAMIC_IS_EMPTY
        .iter()
        .all(|s| resolve_configured_addr(s).is_empty()));
}

#[test]
fn test_configured_address_without_port_is_ignored() {
    assert!(resolve_configured_addr("192.168.1.9").is_empty());
}

#[test]
fn test_no_address_at_all_gives_no_targets() {
    let id = DeviceId([0x31; 32]);
    let (resolved, targets) =
        collect_peer_targets(&device(id, "dynamic"), &discovered(vec![]), &global(vec![]));
    assert_eq!(resolved, id);
    assert!(targets.is_empty());
}

#[test]
fn test_order_is_configured_then_discovered_then_global_direct_then_relays() {
    let id = DeviceId([0x41; 32]);
    let relay_id = DeviceId([0x42; 32]);
    let configured = sock("10.0.0.1:22000");
    let lan = sock("10.0.0.2:22000");
    let wan = sock("203.0.113.7:22000");
    let relay_a = sock("198.51.100.1:22067");
    let relay_b = sock("198.51.100.2:22067");

    let (_, targets) = collect_peer_targets(
        &device(id, "10.0.0.1:22000"),
        &discovered(vec![(id, vec![lan])]),
        &global(vec![(
            id,
            vec![
                PeerEndpoint::Relay {
                    addr: relay_a,
                    device_id: relay_id,
                },
                PeerEndpoint::Direct(wan),
                PeerEndpoint::Relay {
                    addr: relay_b,
                    device_id: relay_id,
                },
            ],
        )]),
    );
    assert_eq!(
        targets,
        vec![
            PeerEndpoint::Direct(configured),
            PeerEndpoint::Direct(lan),
            PeerEndpoint::Direct(wan),
            PeerEndpoint::Relay {
                addr: relay_a,
                device_id: relay_id
            },
            PeerEndpoint::Relay {
                addr: relay_b,
                device_id: relay_id
            },
        ]
    );
}

#[test]
fn test_duplicate_endpoints_are_kept_once_across_sources() {
    let id = DeviceId([0x51; 32]);
    let addr = sock("10.0.0.5:22000");
    let (_, targets) = collect_peer_targets(
        &device(id, "10.0.0.5:22000"),
        &discovered(vec![(id, vec![addr, addr])]),
        &global(vec![(
            id,
            vec![PeerEndpoint::Direct(addr), PeerEndpoint::Direct(addr)],
        )]),
    );
    assert_eq!(targets, vec![PeerEndpoint::Direct(addr)]);
}

#[test]
fn test_endpoints_of_other_devices_are_ignored() {
    let id = DeviceId([0x61; 32]);
    let other = DeviceId([0x62; 32]);
    let stranger = sock("10.9.9.9:22000");
    let (resolved, targets) = collect_peer_targets(
        &device(id, "dynamic"),
        &discovered(vec![(other, vec![stranger])]),
        &global(vec![(other, vec![PeerEndpoint::Direct(stranger)])]),
    );
    assert_eq!(resolved, id, "the configured identity is kept");
    assert!(targets.is_empty());
}

#[test]
fn test_short_id_placeholder_is_resolved_to_the_full_discovered_id() {
    let full = DeviceId([0x71; 32]);
    let short_text = full.to_syncthing_string()[..7].to_owned();
    let placeholder = DeviceId::parse_user_id(&short_text).unwrap();
    assert!(placeholder.is_short_placeholder());
    let lan = sock("10.0.0.7:22000");

    let (resolved, targets) = collect_peer_targets(
        &device(placeholder, "dynamic"),
        &discovered(vec![(full, vec![lan])]),
        &global(vec![]),
    );
    assert_eq!(resolved, full, "the full identity replaces the placeholder");
    assert_eq!(targets, vec![PeerEndpoint::Direct(lan)]);
}

#[test]
fn test_global_lookup_skips_placeholders_and_connected_devices_only() {
    let full = DeviceId([0x81; 32]);
    let placeholder = DeviceId::parse_user_id("MFZWI3D").unwrap();
    assert!(needs_global_lookup(&device(full, "dynamic"), &[]));
    assert!(!needs_global_lookup(&device(full, "dynamic"), &[full]));
    assert!(!needs_global_lookup(&device(placeholder, "dynamic"), &[]));
    // Connected to another device only: still looked up.
    assert!(needs_global_lookup(
        &device(full, "dynamic"),
        &[DeviceId([0x82; 32])]
    ));
}

#[test]
fn test_lan_discovered_comes_before_non_local_configured_address() {
    let id = DeviceId([0x83; 32]);
    let lan = sock("192.168.1.50:22000");
    let public_configured = sock("203.0.113.7:22000");
    let (_, targets) = collect_peer_targets(
        &device(id, "203.0.113.7:22000"),
        &discovered(vec![(id, vec![lan])]),
        &global(vec![]),
    );
    assert_eq!(
        targets,
        vec![
            PeerEndpoint::Direct(lan),
            PeerEndpoint::Direct(public_configured),
        ]
    );
}

#[test]
fn test_global_local_ip_comes_before_global_wan_ip() {
    let id = DeviceId([0x84; 32]);
    let wan = sock("203.0.113.8:22000");
    let lan = sock("192.168.1.60:22000");
    let (_, targets) = collect_peer_targets(
        &device(id, "dynamic"),
        &discovered(vec![]),
        &global(vec![(
            id,
            vec![PeerEndpoint::Direct(wan), PeerEndpoint::Direct(lan)],
        )]),
    );
    assert_eq!(
        targets,
        vec![PeerEndpoint::Direct(lan), PeerEndpoint::Direct(wan)]
    );
}
