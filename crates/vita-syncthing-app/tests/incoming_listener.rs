mod common;

use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use syncthing_core::tls::polling::is_socket_timeout_or_would_block;
use syncthing_core::{
    connect_and_sync, BepHello, DeviceIdentity, FsSyncStorage, SessionParams, SyncControl,
    SyncEvent,
};
use vita_syncthing_app::domain::KnownDevice;
use vita_syncthing_app::infrastructure::incoming_listener::{
    bind_bep_listener, run_tcp_listener, IncomingListenerContext,
};
use vita_syncthing_app::infrastructure::session_guard::{
    ConnectionSlots, SessionClaims, UnknownDeviceLog,
};
use vita_syncthing_app::infrastructure::WorkerMsg;

fn identity_from(byte: u8) -> DeviceIdentity {
    DeviceIdentity::from_seed(&[byte; 32]).unwrap()
}

fn wait_for_msg<F, T>(rx: &Receiver<WorkerMsg>, timeout: Duration, mut pred: F) -> Option<T>
where
    F: FnMut(WorkerMsg) -> Option<T>,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(msg) => {
                if let Some(found) = pred(msg) {
                    return Some(found);
                }
            }
            Err(_) => return None,
        }
    }
    None
}

#[test]
fn bind_bep_listener_ephemeral_is_nonblocking() {
    let (listener, port) = bind_bep_listener(0).expect("bind ephemeral");
    assert!(port > 0);
    assert_eq!(listener.local_addr().unwrap().port(), port);

    let err = listener
        .accept()
        .expect_err("non-blocking accept with no peer must return WouldBlock");
    assert!(is_socket_timeout_or_would_block(&err));
}

#[test]
fn bind_bep_listener_falls_back_when_preferred_port_busy() {
    let occupier = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0))).unwrap();
    let busy_port = occupier.local_addr().unwrap().port();

    let (listener, fallback_port) = bind_bep_listener(busy_port).expect("fallback bind");
    assert!(fallback_port > 0);
    assert_ne!(fallback_port, busy_port);
    assert_eq!(listener.local_addr().unwrap().port(), fallback_port);
    drop(occupier);
}

#[test]
fn incoming_unknown_peer_emits_device_offered_once_with_ip_only() {
    let srv_dir = common::temp_test_dir("in-unknown-srv");
    let cli_dir = common::temp_test_dir("in-unknown-cli");
    let server_id = identity_from(1);
    let unknown_id = identity_from(2);

    let (listener, port) = bind_bep_listener(0).unwrap();
    let control = SyncControl::new();
    let (tx, rx) = mpsc::channel();

    let ctx = IncomingListenerContext {
        identity: server_id.clone(),
        hello: BepHello::new("PS-Vita", "vita-syncthing", "0.1.0"),
        storage: FsSyncStorage::new(&srv_dir),
        approved_devices: Vec::new(),
        claims: Arc::new(SessionClaims::new(4)),
        secondary_inbox: Arc::new(syncthing_core::SecondaryInbox::new()),
        slots: Arc::new(ConnectionSlots::new(2)),
        unknown_log: Arc::new(UnknownDeviceLog::new(16)),
        control: Arc::clone(&control),
        tx,
    };

    let srv_handle = thread::spawn(move || run_tcp_listener(listener, ctx));
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // First dial from unknown peer: server must emit DeviceOffered with IP-only address.
    let cli_storage = FsSyncStorage::new(&cli_dir);
    let cli_hello = BepHello::new("Unknown", "syncthing-cli", "0.1.0");
    let params = SessionParams {
        identity: &unknown_id,
        hello: &cli_hello,
        storage: &cli_storage,
        expected_device_id: Some(server_id.device_id),
        timeout: Duration::from_secs(2),
        control: None,
        secondary_inbox: None,
    };
    let _ = connect_and_sync(&params, addr, |_| {});

    let offer = wait_for_msg(&rx, Duration::from_secs(3), |msg| match msg {
        WorkerMsg::DeviceOffered {
            device_id,
            address,
            via_relay,
        } => Some((device_id, address, via_relay)),
        _ => None,
    })
    .expect("expected DeviceOffered for unknown peer");

    assert_eq!(offer.0, unknown_id.device_id);
    assert_eq!(offer.1, "127.0.0.1");
    assert!(!offer.2);

    // Second dial from the same unknown peer must be suppressed by UnknownDeviceLog.
    let _ = connect_and_sync(&params, addr, |_| {});
    let duplicate = wait_for_msg(&rx, Duration::from_millis(400), |msg| match msg {
        WorkerMsg::DeviceOffered { .. } => Some(()),
        _ => None,
    });
    assert!(
        duplicate.is_none(),
        "duplicate DeviceOffered must be suppressed"
    );

    control.request_stop();
    srv_handle.join().unwrap();
    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
}

#[test]
fn incoming_self_connection_is_rejected_without_offer() {
    let srv_dir = common::temp_test_dir("in-self-srv");
    let cli_dir = common::temp_test_dir("in-self-cli");
    let server_id = identity_from(3);

    let (listener, port) = bind_bep_listener(0).unwrap();
    let control = SyncControl::new();
    let (tx, rx) = mpsc::channel();

    let ctx = IncomingListenerContext {
        identity: server_id.clone(),
        hello: BepHello::new("PS-Vita", "vita-syncthing", "0.1.0"),
        storage: FsSyncStorage::new(&srv_dir),
        approved_devices: Vec::new(),
        claims: Arc::new(SessionClaims::new(4)),
        secondary_inbox: Arc::new(syncthing_core::SecondaryInbox::new()),
        slots: Arc::new(ConnectionSlots::new(2)),
        unknown_log: Arc::new(UnknownDeviceLog::new(16)),
        control: Arc::clone(&control),
        tx,
    };

    let srv_handle = thread::spawn(move || run_tcp_listener(listener, ctx));
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let cli_storage = FsSyncStorage::new(&cli_dir);
    let cli_hello = BepHello::new("Self", "vita-syncthing", "0.1.0");
    let params = SessionParams {
        identity: &server_id,
        hello: &cli_hello,
        storage: &cli_storage,
        expected_device_id: Some(server_id.device_id),
        timeout: Duration::from_secs(2),
        control: None,
        secondary_inbox: None,
    };
    let _ = connect_and_sync(&params, addr, |_| {});

    let unexpected = wait_for_msg(&rx, Duration::from_millis(400), |msg| match msg {
        WorkerMsg::DeviceOffered { .. } | WorkerMsg::InboundSyncEvent(_) => Some(()),
        _ => None,
    });
    assert!(
        unexpected.is_none(),
        "self-connection must never trigger DeviceOffered or InboundSyncEvent"
    );

    control.request_stop();
    srv_handle.join().unwrap();
    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
}

#[test]
fn incoming_approved_peer_runs_sync_and_joins_on_stop() {
    let srv_dir = common::temp_test_dir("in-approved-srv");
    let cli_dir = common::temp_test_dir("in-approved-cli");
    let server_id = identity_from(4);
    let peer_id = identity_from(5);

    let (listener, port) = bind_bep_listener(0).unwrap();
    let srv_control = SyncControl::new();
    let (tx, rx) = mpsc::channel();

    let ctx = IncomingListenerContext {
        identity: server_id.clone(),
        hello: BepHello::new("PS-Vita", "vita-syncthing", "0.1.0"),
        storage: FsSyncStorage::new(&srv_dir),
        approved_devices: vec![KnownDevice {
            device_id: peer_id.device_id,
            name: "Peer".to_owned(),
            address: "dynamic".to_owned(),
        }],
        claims: Arc::new(SessionClaims::new(4)),
        secondary_inbox: Arc::new(syncthing_core::SecondaryInbox::new()),
        slots: Arc::new(ConnectionSlots::new(2)),
        unknown_log: Arc::new(UnknownDeviceLog::new(16)),
        control: Arc::clone(&srv_control),
        tx,
    };

    let srv_handle = thread::spawn(move || run_tcp_listener(listener, ctx));
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let cli_control = SyncControl::new();
    let cli_control_thread = Arc::clone(&cli_control);
    let cli_dir_thread = cli_dir.clone();
    let server_dev_id = server_id.device_id;
    let peer_id_clone = peer_id.clone();

    let cli_handle = thread::spawn(move || {
        let cli_storage = FsSyncStorage::new(&cli_dir_thread);
        let cli_hello = BepHello::new("Peer", "syncthing-cli", "0.1.0");
        let params = SessionParams {
            identity: &peer_id_clone,
            hello: &cli_hello,
            storage: &cli_storage,
            expected_device_id: Some(server_dev_id),
            timeout: Duration::from_secs(3),
            control: Some(cli_control_thread),
            secondary_inbox: None,
        };
        let _ = connect_and_sync(&params, addr, |_| {});
    });

    let connected_peer = wait_for_msg(&rx, Duration::from_secs(5), |msg| match msg {
        WorkerMsg::InboundSyncEvent(SyncEvent::PeerConnected { peer_device_id, .. }) => {
            Some(peer_device_id)
        }
        _ => None,
    })
    .expect("expected InboundSyncEvent(PeerConnected) for approved peer");

    assert_eq!(connected_peer, peer_id.device_id);

    cli_control.request_stop();
    srv_control.request_stop();
    cli_handle.join().unwrap();
    srv_handle.join().unwrap();
    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
}

#[test]
fn incoming_already_claimed_peer_is_skipped() {
    let srv_dir = common::temp_test_dir("in-claimed-srv");
    let cli_dir = common::temp_test_dir("in-claimed-cli");
    let server_id = identity_from(6);
    let peer_id = identity_from(7);

    let (listener, port) = bind_bep_listener(0).unwrap();
    let srv_control = SyncControl::new();
    let (tx, rx) = mpsc::channel();
    let claims = Arc::new(SessionClaims::new(4));
    let _existing_claim = claims.try_claim(peer_id.device_id).unwrap();

    let ctx = IncomingListenerContext {
        identity: server_id.clone(),
        hello: BepHello::new("PS-Vita", "vita-syncthing", "0.1.0"),
        storage: FsSyncStorage::new(&srv_dir),
        approved_devices: vec![KnownDevice {
            device_id: peer_id.device_id,
            name: "Peer".to_owned(),
            address: "dynamic".to_owned(),
        }],
        claims: Arc::clone(&claims),
        secondary_inbox: Arc::new(syncthing_core::SecondaryInbox::new()),
        slots: Arc::new(ConnectionSlots::new(2)),
        unknown_log: Arc::new(UnknownDeviceLog::new(16)),
        control: Arc::clone(&srv_control),
        tx,
    };

    let srv_handle = thread::spawn(move || run_tcp_listener(listener, ctx));
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let cli_storage = FsSyncStorage::new(&cli_dir);
    let cli_hello = BepHello::new("Peer", "syncthing-cli", "0.1.0");
    let params = SessionParams {
        identity: &peer_id,
        hello: &cli_hello,
        storage: &cli_storage,
        expected_device_id: Some(server_id.device_id),
        timeout: Duration::from_secs(2),
        control: None,
        secondary_inbox: None,
    };
    let _ = connect_and_sync(&params, addr, |_| {});

    let unexpected = wait_for_msg(&rx, Duration::from_millis(400), |msg| match msg {
        WorkerMsg::InboundSyncEvent(_) | WorkerMsg::DeviceOffered { .. } => Some(()),
        _ => None,
    });
    assert!(
        unexpected.is_none(),
        "already-claimed peer must not start a duplicate session"
    );
    assert!(claims.is_claimed(&peer_id.device_id));

    srv_control.request_stop();
    srv_handle.join().unwrap();
    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
}
