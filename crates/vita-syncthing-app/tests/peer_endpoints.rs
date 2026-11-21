//! Host integration tests for DNS address resolution, endpoint ordering, dialability gating,
//! Global Discovery retry pacing, and peer session loop fast fallback.

use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, BepHello, DeviceId, DeviceIdentity, FsSyncStorage,
    PeerEndpoint, SessionParams, SyncControl, SyncEvent,
};
use vita_syncthing_app::domain::defaults::{
    GLOBAL_DISCOVERY_INTERVAL, GLOBAL_DISCOVERY_RETRY, RETRY_AFTER_ERROR,
};
use vita_syncthing_app::domain::KnownDevice;
use vita_syncthing_app::infrastructure::peer_endpoints::{
    collect_peer_targets, dialable_targets, is_local_endpoint, next_global_lookup_delay,
    resolve_configured_addr, run_peer_session_loop, PeerSessionShared, SharedDiscoveredAddrs,
    SharedGlobalEndpoints,
};
use vita_syncthing_app::infrastructure::session_guard::SessionClaims;
use vita_syncthing_app::infrastructure::WorkerMsg;

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

#[test]
fn test_is_local_endpoint_and_dialable_targets_gate_non_local_until_initial_lan_done() {
    let lan_ep = PeerEndpoint::Direct("192.168.1.10:22000".parse().unwrap());
    let loopback_ep = PeerEndpoint::Direct("127.0.0.1:22000".parse().unwrap());
    let wan_ep = PeerEndpoint::Direct("203.0.113.10:22000".parse().unwrap());
    let relay_ep = PeerEndpoint::Relay {
        addr: "198.51.100.5:22067".parse().unwrap(),
        device_id: DeviceId([0x33; 32]),
    };

    assert!(is_local_endpoint(&lan_ep));
    assert!(is_local_endpoint(&loopback_ep));
    assert!(!is_local_endpoint(&wan_ep));
    assert!(!is_local_endpoint(&relay_ep));

    let all = vec![
        lan_ep.clone(),
        wan_ep.clone(),
        relay_ep.clone(),
        loopback_ep.clone(),
    ];
    assert_eq!(
        dialable_targets(&all, false),
        vec![lan_ep.clone(), loopback_ep.clone()]
    );
    assert_eq!(dialable_targets(&all, true), all);
}

#[test]
fn test_next_global_lookup_delay_uses_fast_retry_on_error_or_empty_and_interval_otherwise() {
    let peer_a = DeviceId([0x11; 32]);
    let peer_b = DeviceId([0x22; 32]);
    let short = DeviceId::parse_user_id("MFZWI3D").unwrap();
    let kd = |device_id| KnownDevice {
        device_id,
        name: "Peer".to_owned(),
        address: "dynamic".to_owned(),
    };
    let ep_a = PeerEndpoint::Direct("203.0.113.10:22000".parse().unwrap());
    let ep_b = PeerEndpoint::Direct("203.0.113.20:22000".parse().unwrap());

    // Missing entry in global for an unconnected peer -> fast retry.
    let global: SharedGlobalEndpoints = Arc::new(Mutex::new(Vec::new()));
    assert_eq!(
        next_global_lookup_delay(&[kd(peer_a)], &[], &global, &[]),
        GLOBAL_DISCOVERY_RETRY
    );

    // Entry present with empty vec -> fast retry.
    global.lock().unwrap().push((peer_a, Vec::new()));
    assert_eq!(
        next_global_lookup_delay(&[kd(peer_a)], &[], &global, &[]),
        GLOBAL_DISCOVERY_RETRY
    );

    // Entry present with endpoints and no error -> normal interval.
    global.lock().unwrap()[0].1 = vec![ep_a.clone()];
    assert_eq!(
        next_global_lookup_delay(&[kd(peer_a)], &[], &global, &[]),
        GLOBAL_DISCOVERY_INTERVAL
    );

    // Entry present with endpoints, but non-empty last_errors -> fast retry.
    assert_eq!(
        next_global_lookup_delay(
            &[kd(peer_a)],
            &[],
            &global,
            &[(peer_a, "timeout".to_owned())]
        ),
        GLOBAL_DISCOVERY_RETRY
    );

    // Cleared (empty string) error entry -> normal interval.
    assert_eq!(
        next_global_lookup_delay(&[kd(peer_a)], &[], &global, &[(peer_a, String::new())]),
        GLOBAL_DISCOVERY_INTERVAL
    );

    // Second peer unconnected without endpoints -> fast retry; once connected -> normal interval.
    assert_eq!(
        next_global_lookup_delay(&[kd(peer_a), kd(peer_b)], &[], &global, &[]),
        GLOBAL_DISCOVERY_RETRY
    );
    assert_eq!(
        next_global_lookup_delay(&[kd(peer_a), kd(peer_b)], &[peer_b], &global, &[]),
        GLOBAL_DISCOVERY_INTERVAL
    );

    // Short placeholder peers do not trigger fast retry even with empty global cache.
    assert_eq!(
        next_global_lookup_delay(&[kd(short), kd(peer_a)], &[], &global, &[]),
        GLOBAL_DISCOVERY_INTERVAL
    );
    global.lock().unwrap().push((peer_b, vec![ep_b]));
    assert_eq!(
        next_global_lookup_delay(&[kd(peer_a), kd(peer_b)], &[], &global, &[]),
        GLOBAL_DISCOVERY_INTERVAL
    );
}

#[test]
fn test_peer_session_loop_dials_newly_arrived_global_endpoint_without_retry_sleep() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("vita-fast-fallback-{nanos}"));
    let vita_dir = base.join("vita");
    let server_dir = base.join("server");
    std::fs::create_dir_all(&vita_dir).unwrap();
    std::fs::create_dir_all(&server_dir).unwrap();

    let vita_id = DeviceIdentity::from_seed(&[0xC1; 32]).unwrap();
    let server_id = DeviceIdentity::from_seed(&[0xC2; 32]).unwrap();

    // 1. Stale local listener that accepts and holds the socket for 250ms then closes it,
    //    simulating a stale LAN IP failing after Global Discovery arrives mid-dial.
    let stale_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stale_addr = stale_listener.local_addr().unwrap();
    stale_listener.set_nonblocking(true).unwrap();
    let stale_accepts = Arc::new(AtomicUsize::new(0));
    let stale_accepts_bg = Arc::clone(&stale_accepts);
    let stale_stop = Arc::new(AtomicBool::new(false));
    let stale_stop_bg = Arc::clone(&stale_stop);
    let stale_handle = thread::spawn(move || {
        while !stale_stop_bg.load(Ordering::Acquire) {
            match stale_listener.accept() {
                Ok((stream, _)) => {
                    stale_accepts_bg.fetch_add(1, Ordering::AcqRel);
                    thread::sleep(Duration::from_millis(250));
                    drop(stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    // 2. Real BEP/1.0 server representing the Global Discovery endpoint.
    let server_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_addr = server_listener.local_addr().unwrap();
    server_listener.set_nonblocking(true).unwrap();
    let server_control = SyncControl::new();
    let server_control_bg = Arc::clone(&server_control);
    let server_id_bg = server_id.clone();
    let vita_dev_id = vita_id.device_id;
    let mut server_storage = FsSyncStorage::new(&server_dir);
    server_storage.add_known_device(vita_dev_id);
    let server_handle = thread::spawn(move || {
        let hello = BepHello::new("server", "syncthing", "1.27.0");
        while !server_control_bg.is_stop_requested() {
            match server_listener.accept() {
                Ok((tcp, peer_addr)) => {
                    if let Ok(incoming) =
                        accept_bep_tls_stream(&server_id_bg, tcp, Duration::from_secs(5))
                    {
                        let params = SessionParams {
                            identity: &server_id_bg,
                            hello: &hello,
                            storage: &server_storage,
                            expected_device_id: Some(vita_dev_id),
                            timeout: Duration::from_secs(5),
                            control: Some(Arc::clone(&server_control_bg)),
                            secondary_inbox: None,
                        };
                        let _ = accept_and_sync(&params, incoming, peer_addr, |_| {});
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    let kd = KnownDevice {
        device_id: server_id.device_id,
        name: "Server".to_owned(),
        address: stale_addr.to_string(),
    };
    let discovered: SharedDiscoveredAddrs = Arc::new(Mutex::new(Vec::new()));
    let global: SharedGlobalEndpoints = Arc::new(Mutex::new(Vec::new()));
    let claims = Arc::new(SessionClaims::new(4));
    let initial_lan_done = Arc::new(AtomicBool::new(true));
    let client_control = SyncControl::new();
    let (tx, rx) = mpsc::channel();
    let mut vita_storage = FsSyncStorage::new(&vita_dir);
    vita_storage.add_known_device(server_id.device_id);

    let shared = PeerSessionShared {
        identity: vita_id,
        hello: BepHello::new("PS-Vita", "vita-syncthing", "0.1.0"),
        storage: vita_storage,
        discovered,
        global: Arc::clone(&global),
        claims,
        initial_lan_done,
        control: Arc::clone(&client_control),
        tx,
    };

    let start = Instant::now();
    let client_handle = thread::spawn(move || run_peer_session_loop(kd, shared));

    // Wait until the stale LAN dial has started, then publish the Global Discovery endpoint
    // before the stale dial finishes failing.
    let wait_accept = Instant::now() + Duration::from_secs(3);
    while stale_accepts.load(Ordering::Acquire) == 0 {
        assert!(Instant::now() < wait_accept, "stale LAN was never dialed");
        thread::sleep(Duration::from_millis(10));
    }
    global
        .lock()
        .unwrap()
        .push((server_id.device_id, vec![PeerEndpoint::Direct(server_addr)]));

    let mut connected_at = None;
    let deadline = Instant::now() + RETRY_AFTER_ERROR;
    while Instant::now() < deadline {
        while let Ok(msg) = rx.try_recv() {
            if let WorkerMsg::SyncEvent(SyncEvent::PeerConnected { peer_addr, .. }) = msg {
                assert_eq!(peer_addr, server_addr);
                connected_at = Some(start.elapsed());
                break;
            }
        }
        if connected_at.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    client_control.request_stop();
    server_control.request_stop();
    stale_stop.store(true, Ordering::Release);
    let _ = client_handle.join();
    let _ = server_handle.join();
    let _ = stale_handle.join();
    let _ = std::fs::remove_dir_all(&base);

    let elapsed = connected_at.expect("did not connect promptly via newly arrived global endpoint");
    assert!(
        elapsed < RETRY_AFTER_ERROR,
        "expected connection well under RETRY_AFTER_ERROR ({RETRY_AFTER_ERROR:?}), took {elapsed:?}"
    );
    assert_eq!(
        stale_accepts.load(Ordering::Acquire),
        1,
        "stale LAN IP must not be re-dialed before trying newly arrived global endpoint"
    );
}

#[test]
fn test_peer_session_loop_refuses_non_local_endpoints_while_initial_lan_done_is_false() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("vita-lan-gate-{nanos}"));
    std::fs::create_dir_all(&base).unwrap();

    let vita_id = DeviceIdentity::from_seed(&[0xD1; 32]).unwrap();
    let peer_id = DeviceIdentity::from_seed(&[0xD2; 32]).unwrap();

    // Closed local port so the stale configured LAN IP fails immediately.
    let stale_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stale_addr = stale_listener.local_addr().unwrap();
    drop(stale_listener);

    // Non-local endpoint (RFC 5737 TEST-NET-3) present in `global` from the start.
    let non_local_addr: SocketAddr = "203.0.113.42:22000".parse().unwrap();

    let kd = KnownDevice {
        device_id: peer_id.device_id,
        name: "Peer".to_owned(),
        address: stale_addr.to_string(),
    };
    let discovered: SharedDiscoveredAddrs = Arc::new(Mutex::new(Vec::new()));
    let global: SharedGlobalEndpoints = Arc::new(Mutex::new(vec![(
        peer_id.device_id,
        vec![PeerEndpoint::Direct(non_local_addr)],
    )]));
    let claims = Arc::new(SessionClaims::new(4));
    let initial_lan_done = Arc::new(AtomicBool::new(false));
    let client_control = SyncControl::new();
    let (tx, rx) = mpsc::channel();
    let mut vita_storage = FsSyncStorage::new(&base);
    vita_storage.add_known_device(peer_id.device_id);

    let shared = PeerSessionShared {
        identity: vita_id,
        hello: BepHello::new("PS-Vita", "vita-syncthing", "0.1.0"),
        storage: vita_storage,
        discovered,
        global,
        claims: Arc::clone(&claims),
        initial_lan_done: Arc::clone(&initial_lan_done),
        control: Arc::clone(&client_control),
        tx,
    };

    let client_handle = thread::spawn(move || run_peer_session_loop(kd, shared));

    // Wait for the stale local IP failure log while `initial_lan_done` is still `false`.
    let mut logs = Vec::new();
    let wait_stale = Instant::now() + Duration::from_secs(2);
    while Instant::now() < wait_stale {
        while let Ok(msg) = rx.try_recv() {
            if let WorkerMsg::Log(line) = msg {
                logs.push(line);
            }
        }
        if logs.iter().any(|l| l.contains(&stale_addr.to_string())) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        logs.iter().any(|l| l.contains(&stale_addr.to_string())),
        "stale local IP should be attempted immediately: {logs:?}"
    );

    // Stay with `initial_lan_done = false` for 500ms: non-local endpoint must NOT be dialed.
    thread::sleep(Duration::from_millis(500));
    while let Ok(msg) = rx.try_recv() {
        if let WorkerMsg::Log(line) = msg {
            logs.push(line);
        }
    }
    assert!(
        !logs.iter().any(|l| l.contains(&non_local_addr.to_string())),
        "non-local endpoint must not be dialed while initial_lan_done is false: {logs:?}"
    );

    // Once `initial_lan_done` becomes `true`, the non-local endpoint becomes dialable and
    // the session loop wakes up promptly to claim and dial it (well under RETRY_AFTER_ERROR).
    initial_lan_done.store(true, Ordering::Release);
    let wake_deadline = Instant::now() + Duration::from_secs(2);
    let mut non_local_started = false;
    while Instant::now() < wake_deadline {
        if claims.is_claimed(&peer_id.device_id) {
            non_local_started = true;
            break;
        }
        while let Ok(msg) = rx.try_recv() {
            if let WorkerMsg::Log(line) = msg {
                if line.contains(&non_local_addr.to_string()) {
                    non_local_started = true;
                }
                logs.push(line);
            }
        }
        if non_local_started {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    client_control.request_stop();
    let _ = client_handle.join();
    let _ = std::fs::remove_dir_all(&base);

    assert!(
        non_local_started,
        "non-local endpoint should start dialing promptly once initial_lan_done becomes true: {logs:?}"
    );
}
