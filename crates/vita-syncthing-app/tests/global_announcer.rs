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

#[test]
fn lan_announce_uses_explicit_local_ipv4_so_docker_nat_never_dials_bridge_gateway() {
    use std::net::{Ipv4Addr, SocketAddr};
    use syncthing_core::resolve_peer_tcp_addrs;
    use vita_syncthing_app::infrastructure::sync_worker::lan_announce_addresses;

    let vita_lan_ip = Ipv4Addr::new(192, 168, 1, 42);
    let addrs = lan_announce_addresses(Some(22000), Some(vita_lan_ip));
    assert_eq!(addrs, vec!["tcp://192.168.1.42:22000"]);

    // When the UDP 21027 broadcast crosses a Docker bridge, Docker rewrites the UDP source IP
    // to the bridge gateway (e.g. 172.24.0.1). Because the Vita advertised its explicit LAN IP
    // instead of 0.0.0.0, the peer resolves 192.168.1.42:22000 and never dials 172.24.0.1:22000.
    let docker_gateway: SocketAddr = "172.24.0.1:21027".parse().unwrap();
    let resolved = resolve_peer_tcp_addrs(docker_gateway, &addrs);
    assert_eq!(resolved, vec!["192.168.1.42:22000".parse().unwrap()]);
}

#[test]
fn lan_announce_omits_address_when_local_ip_unknown_or_invalid() {
    use std::net::Ipv4Addr;
    use vita_syncthing_app::infrastructure::sync_worker::lan_announce_addresses;

    // Never fall back to `tcp://0.0.0.0:22000`, which causes Docker peers to dial their own gateway.
    assert!(lan_announce_addresses(Some(22000), None).is_empty());
    assert!(lan_announce_addresses(Some(22000), Some(Ipv4Addr::UNSPECIFIED)).is_empty());
    assert!(lan_announce_addresses(Some(22000), Some(Ipv4Addr::LOCALHOST)).is_empty());
    assert!(lan_announce_addresses(Some(0), Some(Ipv4Addr::new(192, 168, 1, 42))).is_empty());
    assert!(lan_announce_addresses(None, Some(Ipv4Addr::new(192, 168, 1, 42))).is_empty());
}

#[test]
fn global_announce_omits_unmapped_tcp_behind_nat_and_uses_external_port_when_mapped() {
    use vita_syncthing_app::infrastructure::sync_worker::global_announce_port;

    // Behind NAT without a UPnP/NAT-PMP port mapping, advertising `tcp://0.0.0.0:22000` causes
    // Global Discovery to publish `<home_public_ip>:22000`, which is unreachable or hits another
    // forwarded device. Only the relay URI must be announced.
    let port = global_announce_port(Some(22000), None);
    assert_eq!(port, None);
    assert_eq!(
        build_announced_addresses(port, Some("relay://188.93.140.100:22067/?id=RELAY")),
        vec!["relay://188.93.140.100:22067/?id=RELAY"]
    );

    // When a real external port mapping exists (UPnP/NAT-PMP), announce that external port.
    let mapped = global_announce_port(Some(22000), Some(41234));
    assert_eq!(mapped, Some(41234));
    assert_eq!(
        build_announced_addresses(mapped, Some("relay://188.93.140.100:22067/?id=RELAY")),
        vec![
            "tcp://0.0.0.0:41234",
            "relay://188.93.140.100:22067/?id=RELAY"
        ]
    );

    // Zero external port is ignored.
    assert_eq!(global_announce_port(Some(22000), Some(0)), None);
}

#[test]
fn global_announcer_wakes_promptly_when_shared_relay_uri_changes() {
    use std::net::TcpListener;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};
    use syncthing_core::{DeviceIdentity, HttpsClient, HttpsServer, SyncControl};
    use vita_syncthing_app::domain::defaults::GLOBAL_ANNOUNCE_POLL;
    use vita_syncthing_app::infrastructure::global_announcer::{
        run_global_announcer_on, GlobalAnnouncerContext,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();

    let server = HttpsServer::parse(&format!("https://127.0.0.1:{port}/v2/")).unwrap();
    let control = SyncControl::new();
    let shared_relay_uri = Arc::new(Mutex::new(None));
    let (tx, _rx) = mpsc::channel();

    let ctx = GlobalAnnouncerContext {
        identity: DeviceIdentity::from_seed(&[0x55; 32]).unwrap(),
        listen_port: Some(22000),
        shared_relay_uri: Arc::clone(&shared_relay_uri),
        control: Arc::clone(&control),
        tx,
    };

    let handle = std::thread::spawn(move || {
        run_global_announcer_on(&[server], &HttpsClient::default(), &ctx);
    });

    // Wait for the initial announce attempt (only tcp://0.0.0.0:22000).
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                drop(stream);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for 1st announce"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("accept failed: {e}"),
        }
    }

    // Set the relay URI while the announcer is sleeping on GLOBAL_ANNOUNCE_POLL (5 s).
    let changed_at = Instant::now();
    *shared_relay_uri.lock().unwrap() = Some("relay://198.51.100.5:22067/?id=RELAY".to_owned());

    // The announcer must wake and re-attempt well under GLOBAL_ANNOUNCE_POLL.
    let wake_deadline = Instant::now() + GLOBAL_ANNOUNCE_POLL;
    let mut second_elapsed = None;
    while Instant::now() < wake_deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                drop(stream);
                second_elapsed = Some(changed_at.elapsed());
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("accept failed: {e}"),
        }
    }

    control.request_stop();
    handle.join().unwrap();

    let elapsed = second_elapsed.expect("announcer did not re-announce after relay URI change");
    assert!(
        elapsed < Duration::from_secs(2),
        "announcer took {elapsed:?} to wake after relay URI change (poll={GLOBAL_ANNOUNCE_POLL:?})"
    );
}
