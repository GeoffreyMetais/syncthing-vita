use syncthing_core::DEFAULT_ANNOUNCE_SERVERS;
use vita_syncthing_app::infrastructure::global_announcer::{
    announce_servers, build_announced_addresses, default_announce_servers,
};

#[test]
fn build_announced_addresses_combinations() {
    assert!(build_announced_addresses(None, None).is_empty());

    assert_eq!(
        build_announced_addresses(Some(22000), None),
        vec!["tcp://0.0.0.0:22000"]
    );

    assert_eq!(
        build_announced_addresses(None, Some("relay://1.2.3.4:22067/?id=ABC")),
        vec!["relay://1.2.3.4:22067/?id=ABC"]
    );

    assert_eq!(
        build_announced_addresses(Some(22000), Some("relay://1.2.3.4:22067/?id=ABC")),
        vec!["tcp://0.0.0.0:22000", "relay://1.2.3.4:22067/?id=ABC"]
    );
}

#[test]
fn build_announced_addresses_ignores_zero_port_and_blank_relay() {
    assert!(build_announced_addresses(Some(0), Some("")).is_empty());
    assert!(build_announced_addresses(Some(0), Some("   ")).is_empty());

    assert_eq!(
        build_announced_addresses(Some(22000), Some("   ")),
        vec!["tcp://0.0.0.0:22000"]
    );

    assert_eq!(
        build_announced_addresses(Some(0), Some("  relay://1.2.3.4:22067/?id=ABC  ")),
        vec!["relay://1.2.3.4:22067/?id=ABC"]
    );
}

#[test]
fn announce_servers_selects_ipv4_only_on_vita_and_both_on_host() {
    let vita_servers = announce_servers(true);
    assert_eq!(vita_servers.len(), 1);
    assert_eq!(vita_servers, &DEFAULT_ANNOUNCE_SERVERS[..1]);
    assert!(vita_servers[0].contains("discovery-announce-v4"));

    let host_servers = announce_servers(false);
    assert_eq!(host_servers.len(), 2);
    assert_eq!(host_servers, &DEFAULT_ANNOUNCE_SERVERS);
    assert!(host_servers[0].contains("discovery-announce-v4"));
    assert!(host_servers[1].contains("discovery-announce-v6"));

    // On x86_64 host tests, default_announce_servers() must return both servers.
    assert_eq!(default_announce_servers(), &DEFAULT_ANNOUNCE_SERVERS);
}
