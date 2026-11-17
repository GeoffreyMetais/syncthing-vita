//! Where the worker looks for a peer, and in which order.

use std::net::SocketAddr;

use syncthing_core::{Announce, DeviceId, DeviceIdentity, DiscoveryCycleResult, PeerEndpoint};
use syncthing_vita_worker::config::DeviceRecord;
use syncthing_vita_worker::endpoints::{endpoint_label, resolve_configured_addr, KnownEndpoints};

fn id(seed: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[seed; 32]).unwrap().device_id
}

fn record(device_id: DeviceId, address: &str) -> DeviceRecord {
    DeviceRecord {
        device_id,
        name: "peer".to_owned(),
        address: address.to_owned(),
    }
}

fn sock(s: &str) -> SocketAddr {
    s.parse().unwrap()
}

#[test]
fn test_configured_addresses() {
    assert!(resolve_configured_addr("").is_empty());
    assert!(resolve_configured_addr("  dynamic ").is_empty());
    assert!(resolve_configured_addr("DYNAMIC").is_empty());
    assert!(resolve_configured_addr("0.0.0.0:22000").is_empty());
    assert!(resolve_configured_addr("not an address").is_empty());
    assert_eq!(
        resolve_configured_addr(" 192.168.1.5:22000 "),
        vec![sock("192.168.1.5:22000")]
    );
    assert_eq!(
        resolve_configured_addr("localhost:22001")
            .into_iter()
            .filter(|a| a.ip().is_loopback())
            .map(|a| a.port())
            .next(),
        Some(22001)
    );
}

#[test]
fn test_order_is_configured_lan_global_then_relays() {
    let peer = id(1);
    let mut known = KnownEndpoints::default();
    known.add_lan(&DiscoveryCycleResult {
        local_port: 21027,
        bytes_sent: 0,
        discovered_peers: vec![
            (
                sock("10.0.0.7:21027"),
                Announce {
                    id: peer,
                    addresses: vec!["tcp://0.0.0.0:22000".to_owned()],
                    instance_id: 1,
                },
            ),
            (
                sock("10.0.0.9:21027"),
                Announce {
                    id: id(2),
                    addresses: vec!["tcp://10.0.0.9:22000".to_owned()],
                    instance_id: 2,
                },
            ),
        ],
    });
    let relay = PeerEndpoint::Relay {
        addr: sock("5.5.5.5:22067"),
        device_id: id(9),
    };
    known.add_global(
        peer,
        vec![
            relay.clone(),
            PeerEndpoint::Direct(sock("8.8.4.4:22000")),
            PeerEndpoint::Direct(sock("192.168.1.5:22000")),
        ],
    );
    let (expected, targets) = known.targets(&record(peer, "192.168.1.5:22000"));
    assert_eq!(expected, peer);
    assert_eq!(
        targets,
        vec![
            PeerEndpoint::Direct(sock("192.168.1.5:22000")),
            PeerEndpoint::Direct(sock("10.0.0.7:22000")),
            PeerEndpoint::Direct(sock("8.8.4.4:22000")),
            relay,
        ],
        "configured first, duplicates removed, relays last"
    );
    // Another device's announcement is never used.
    let (_, other) = known.targets(&record(id(3), "dynamic"));
    assert!(other.is_empty());
}

#[test]
fn test_latest_announcement_wins_and_placeholders_get_the_full_id() {
    let peer = id(4);
    let placeholder = DeviceId::parse_user_id(&peer.short()).unwrap();
    assert!(placeholder.is_short_placeholder());
    let mut known = KnownEndpoints::default();
    let round = |ip: &str| DiscoveryCycleResult {
        local_port: 21027,
        bytes_sent: 0,
        discovered_peers: vec![(
            sock(&format!("{ip}:21027")),
            Announce {
                id: peer,
                addresses: vec!["tcp://0.0.0.0:22000".to_owned()],
                instance_id: 1,
            },
        )],
    };
    known.add_lan(&round("10.0.0.1"));
    known.add_lan(&round("10.0.0.2"));
    let (expected, targets) = known.targets(&record(placeholder, "dynamic"));
    assert_eq!(expected, peer, "the announced full ID is expected in TLS");
    assert_eq!(targets, vec![PeerEndpoint::Direct(sock("10.0.0.2:22000"))]);
    known.add_global(peer, vec![PeerEndpoint::Direct(sock("1.2.3.4:1"))]);
    known.add_global(peer, vec![PeerEndpoint::Direct(sock("1.2.3.4:2"))]);
    let (_, targets) = known.targets(&record(peer, "dynamic"));
    assert_eq!(
        targets,
        vec![
            PeerEndpoint::Direct(sock("10.0.0.2:22000")),
            PeerEndpoint::Direct(sock("1.2.3.4:2")),
        ]
    );
}

#[test]
fn test_labels() {
    assert_eq!(
        endpoint_label(&PeerEndpoint::Direct(sock("1.2.3.4:22000"))),
        "1.2.3.4:22000"
    );
    assert_eq!(
        endpoint_label(&PeerEndpoint::Relay {
            addr: sock("5.6.7.8:22067"),
            device_id: id(5),
        }),
        "relay://5.6.7.8:22067"
    );
}
