//! Integration test verifying the WAN/relay -> LAN reconnection transition on the same peer:
//!
//! 1. A peer connected over a public WAN/relay IP keeps `"dynamic"` in `devices.cfg` (no public IP
//!    or relay endpoint is persisted).
//! 2. `collect_peer_targets` places LAN direct addresses strictly first once discovered, ahead of
//!    both WAN direct and relay endpoints.
//! 3. `run_peer_session_loop` with unreachable WAN/relay endpoints in `global` connects over the
//!    local LAN endpoint (`127.0.0.1:<port>`) as soon as LAN discovery populates `discovered`, and
//!    `SyncCoordinator` observes `PeerConnected` with that local LAN socket address.

mod common;

use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, BepHello, DeviceIdentity, FsSyncStorage, PeerEndpoint,
    SessionParams, SyncControl, SyncEvent, SyncSessionStatus,
};
use vita_syncthing_app::application::SyncCoordinator;
use vita_syncthing_app::domain::{AppState, KnownDevice};
use vita_syncthing_app::infrastructure::peer_endpoints::{
    collect_peer_targets, run_peer_session_loop, PeerSessionShared, SharedDiscoveredAddrs,
    SharedGlobalEndpoints,
};
use vita_syncthing_app::infrastructure::session_guard::SessionClaims;
use vita_syncthing_app::infrastructure::{ConfigRepository, WorkerMsg};

fn identity_with_byte(b: u8) -> DeviceIdentity {
    DeviceIdentity::from_seed(&[b; 32]).unwrap()
}

fn sock(text: &str) -> SocketAddr {
    text.parse().unwrap()
}

#[test]
fn test_wan_connection_keeps_dynamic_address_in_devices_cfg() {
    let dir = common::temp_test_dir("internet-then-lan-dynamic");
    let repo = ConfigRepository::new(&dir);
    let peer_id = identity_with_byte(0x21).device_id;

    let initial_devices = vec![KnownDevice {
        device_id: peer_id,
        name: "linux-cli".to_owned(),
        address: "dynamic".to_owned(),
    }];
    repo.save_devices(&initial_devices);

    let mut state = AppState::new(Vec::new(), initial_devices, "ux0:");
    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);

    // Phase 1: simulate a session established over a public WAN / relay address.
    tx.send(WorkerMsg::SyncEvent(SyncEvent::PeerConnected {
        peer_addr: sock("198.51.100.42:22067"),
        peer_device_id: peer_id,
        peer_name: "linux-cli".to_owned(),
        client_version: "v2.0.0".to_owned(),
    }))
    .unwrap();

    assert!(SyncCoordinator::process_worker_messages(
        &mut state,
        &repo,
        &mut worker_rx,
    ));

    let persisted = repo.load_devices();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].device_id, peer_id);
    assert_eq!(persisted[0].address, "dynamic");
    assert_eq!(state.devices[0].address, "dynamic");

    let cfg_text = std::fs::read_to_string(format!("{dir}/devices.cfg")).unwrap();
    assert!(
        !cfg_text.contains("198.51.100.42"),
        "WAN/relay IP must never be persisted to devices.cfg: {cfg_text}"
    );
    assert!(cfg_text.contains("|dynamic\n"), "cfg: {cfg_text}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_collect_peer_targets_moves_lan_first_when_discovered_after_wan_and_relay() {
    let peer_id = identity_with_byte(0x31).device_id;
    let relay_id = identity_with_byte(0x32).device_id;
    let kd = KnownDevice {
        device_id: peer_id,
        name: "linux-cli".to_owned(),
        address: "dynamic".to_owned(),
    };

    let wan = sock("198.51.100.42:22000");
    let relay = sock("198.51.100.99:22067");
    let lan = sock("192.168.1.50:22000");

    let discovered: SharedDiscoveredAddrs = Arc::new(Mutex::new(Vec::new()));
    let global: SharedGlobalEndpoints = Arc::new(Mutex::new(vec![(
        peer_id,
        vec![
            PeerEndpoint::Relay {
                addr: relay,
                device_id: relay_id,
            },
            PeerEndpoint::Direct(wan),
        ],
    )]));

    // Before LAN discovery: WAN direct comes first, then Relay.
    let (resolved, targets_before) = collect_peer_targets(&kd, &discovered, &global);
    assert_eq!(resolved, peer_id);
    assert_eq!(
        targets_before,
        vec![
            PeerEndpoint::Direct(wan),
            PeerEndpoint::Relay {
                addr: relay,
                device_id: relay_id,
            },
        ]
    );

    // Once LAN discovery observes a local endpoint, LAN direct must be strictly first.
    discovered.lock().unwrap().push((peer_id, vec![lan]));
    let (resolved_after, targets_after) = collect_peer_targets(&kd, &discovered, &global);
    assert_eq!(resolved_after, peer_id);
    assert_eq!(
        targets_after,
        vec![
            PeerEndpoint::Direct(lan),
            PeerEndpoint::Direct(wan),
            PeerEndpoint::Relay {
                addr: relay,
                device_id: relay_id,
            },
        ]
    );
}

#[test]
fn test_peer_session_loop_connects_on_lan_after_lan_discovery_despite_wan_and_relay_endpoints() {
    let dir = common::temp_test_dir("internet-then-lan-live");
    let vita_dir = format!("{dir}/vita");
    let peer_dir = format!("{dir}/peer");
    std::fs::create_dir_all(&vita_dir).unwrap();
    std::fs::create_dir_all(&peer_dir).unwrap();

    let vita_id = identity_with_byte(0x41);
    let peer_id = identity_with_byte(0x42);
    let relay_id = identity_with_byte(0x43).device_id;

    // Bind an ephemeral local TCP listener acting as the peer on the LAN.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let lan_addr = listener.local_addr().unwrap();

    // Unreachable WAN and relay endpoints (RFC 5737 TEST-NET-2) placed in `global`:
    // because LAN discovery provides `127.0.0.1:<port>` first, `run_peer_session_loop`
    // connects immediately to `lan_addr` without touching these unreachable WAN/relay targets.
    let unreachable_wan = sock("198.51.100.42:22000");
    let unreachable_relay = sock("198.51.100.99:22067");

    let peer_storage = FsSyncStorage::new(format!("{peer_dir}/sync"));
    let peer_control = SyncControl::new();
    let server_control = Arc::clone(&peer_control);
    let server_identity = peer_id.clone();
    let expected_vita = vita_id.device_id;

    let server_handle = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && !server_control.is_stop_requested() {
            match listener.accept() {
                Ok((tcp, remote_addr)) => {
                    let incoming =
                        accept_bep_tls_stream(&server_identity, tcp, Duration::from_secs(5))
                            .unwrap();
                    let hello = BepHello::new("linux-cli", "syncthing-cli", "v2.0.0");
                    let params = SessionParams {
                        identity: &server_identity,
                        hello: &hello,
                        storage: &peer_storage,
                        expected_device_id: Some(expected_vita),
                        timeout: Duration::from_secs(5),
                        control: Some(Arc::clone(&server_control)),
                    };
                    let res = accept_and_sync(&params, incoming, remote_addr, |_| {}).unwrap();
                    return res.session_status;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => panic!("listener accept failed: {e}"),
            }
        }
        panic!("server did not receive LAN connection before deadline");
    });

    let repo = ConfigRepository::new(&vita_dir);
    let kd = KnownDevice {
        device_id: peer_id.device_id,
        name: "linux-cli".to_owned(),
        address: "dynamic".to_owned(),
    };
    repo.save_devices(std::slice::from_ref(&kd));

    let discovered: SharedDiscoveredAddrs = Arc::new(Mutex::new(Vec::new()));
    let global: SharedGlobalEndpoints = Arc::new(Mutex::new(vec![(
        peer_id.device_id,
        vec![
            PeerEndpoint::Direct(unreachable_wan),
            PeerEndpoint::Relay {
                addr: unreachable_relay,
                device_id: relay_id,
            },
        ],
    )]));

    // Populate LAN discovery before starting the session loop (mirrors Phase 2 LAN discovery
    // providing the local address alongside previously cached WAN/relay endpoints).
    discovered
        .lock()
        .unwrap()
        .push((peer_id.device_id, vec![lan_addr]));

    let client_control = SyncControl::new();
    let (tx, rx) = mpsc::channel();
    let shared = PeerSessionShared {
        identity: vita_id,
        hello: BepHello::new("PS-Vita", "vita-syncthing", "0.1.0"),
        storage: FsSyncStorage::new(format!("{vita_dir}/sync")),
        discovered: Arc::clone(&discovered),
        global: Arc::clone(&global),
        claims: Arc::new(SessionClaims::new(4)),
        initial_lan_done: Arc::new(AtomicBool::new(true)),
        control: Arc::clone(&client_control),
        tx,
    };

    let client_handle = thread::spawn(move || run_peer_session_loop(kd, shared));

    let mut state = AppState::new(Vec::new(), repo.load_devices(), "ux0:");
    let (fwd_tx, fwd_rx) = mpsc::channel();
    let mut worker_rx = Some(fwd_rx);
    let mut observed_peer_connected_addr: Option<SocketAddr> = None;

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        while let Ok(msg) = rx.try_recv() {
            if let WorkerMsg::SyncEvent(SyncEvent::PeerConnected { peer_addr, .. }) = &msg {
                observed_peer_connected_addr = Some(*peer_addr);
            }
            fwd_tx.send(msg).unwrap();
        }
        SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
        if state.session_connected && observed_peer_connected_addr.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    client_control.request_stop();
    peer_control.request_stop();
    client_handle.join().unwrap();
    let server_status = server_handle.join().unwrap();

    assert_eq!(
        observed_peer_connected_addr,
        Some(lan_addr),
        "PeerConnected must carry the local LAN address; logs={:?}",
        state.logs
    );
    assert!(state.session_connected);
    assert_eq!(repo.load_devices()[0].address, lan_addr.to_string());
    assert!(
        matches!(server_status, SyncSessionStatus::Active { .. }),
        "expected active server session, got {server_status:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
