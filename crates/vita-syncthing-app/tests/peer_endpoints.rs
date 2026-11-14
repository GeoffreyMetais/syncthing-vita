//! Host integration tests for DNS address resolution and peer endpoint ordering.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use syncthing_core::{DeviceId, PeerEndpoint};
use vita_syncthing_app::domain::KnownDevice;
use vita_syncthing_app::infrastructure::peer_endpoints::{
    collect_peer_targets, resolve_configured_addr, SharedDiscoveredAddrs, SharedGlobalEndpoints,
};

#[test]
fn test_resolve_configured_addr_handles_ip_dns_dynamic_and_unspecified() {
    assert!(resolve_configured_addr("").is_empty());
    assert!(resolve_configured_addr("dynamic").is_empty());
    assert!(resolve_configured_addr("0.0.0.0:22000").is_empty());
    assert_eq!(
        resolve_configured_addr("192.168.1.50:22000"),
        vec!["192.168.1.50:22000".parse::<SocketAddr>().unwrap()]
    );
    let local = resolve_configured_addr("localhost:22000");
    assert!(!local.is_empty());
    assert!(local.iter().all(|s| s.port() == 22000));
}

#[test]
fn test_collect_peer_targets_orders_direct_before_relay_and_deduplicates() {
    let peer_id = DeviceId([0x11; 32]);
    let relay_id = DeviceId([0x22; 32]);
    let kd = KnownDevice {
        device_id: peer_id,
        name: "Server".to_owned(),
        address: "192.168.1.67:22000".to_owned(),
    };

    let lan_addr: SocketAddr = "192.168.1.67:22000".parse().unwrap();
    let wan_addr: SocketAddr = "203.0.113.10:22000".parse().unwrap();
    let relay_addr: SocketAddr = "198.51.100.5:22067".parse().unwrap();

    let discovered: SharedDiscoveredAddrs = Arc::new(Mutex::new(vec![(peer_id, vec![lan_addr])]));
    let global: SharedGlobalEndpoints = Arc::new(Mutex::new(vec![(
        peer_id,
        vec![
            PeerEndpoint::Relay {
                addr: relay_addr,
                device_id: relay_id,
            },
            PeerEndpoint::Direct(wan_addr),
            PeerEndpoint::Direct(lan_addr),
        ],
    )]));

    let (resolved_id, targets) = collect_peer_targets(&kd, &discovered, &global);
    assert_eq!(resolved_id, peer_id);
    assert_eq!(
        targets,
        vec![
            PeerEndpoint::Direct(lan_addr),
            PeerEndpoint::Direct(wan_addr),
            PeerEndpoint::Relay {
                addr: relay_addr,
                device_id: relay_id,
            },
        ]
    );
}

/// Global Discovery is only asked about peers that are not connected and have a full ID.
#[test]
fn test_global_lookup_only_for_disconnected_full_ids() {
    use vita_syncthing_app::infrastructure::peer_endpoints::needs_global_lookup;
    let device = |device_id| KnownDevice {
        device_id,
        name: "NAS".to_owned(),
        address: "dynamic".to_owned(),
    };
    let full = DeviceId([0x11; 32]);
    let short = DeviceId::parse_user_id("MFZWI3D").unwrap();

    assert!(needs_global_lookup(&device(full), &[]));
    assert!(!needs_global_lookup(&device(full), &[full]));
    assert!(needs_global_lookup(&device(full), &[DeviceId([0x22; 32])]));
    assert!(!needs_global_lookup(&device(short), &[]));
}
