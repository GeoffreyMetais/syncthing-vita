//! Incoming sessions (`tls::accept_bep_tls_stream` + `session::accept_and_sync`): two
//! `syncthing-core` devices, one listening and one dialing, synchronize a folder both ways.

mod common;

use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, connect_and_sync, BepFolder, BepHello, DeviceId,
    DeviceIdentity, FsSyncStorage, SessionParams, SyncControl, SyncEvent, SyncSessionStatus,
    SyncTuning,
};

const TIMEOUT: Duration = Duration::from_secs(4);
const FOLDER: &str = "shared";

/// Short rescan interval so a file created during the session is published quickly.
fn fast_control() -> Arc<SyncControl> {
    SyncControl::with_tuning(SyncTuning {
        rescan_interval: Duration::from_millis(300),
        keepalive_interval: Duration::from_secs(20),
    })
}

/// A storage sharing `FOLDER` (at `root/data`) with `peer`, announced in our `ClusterConfig`.
fn storage_sharing_with(root: &Path, peer: DeviceId) -> FsSyncStorage {
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let mut storage = FsSyncStorage::new(root.join("db"));
    storage.set_folder_path(FOLDER, &data);
    storage.set_folder_devices(FOLDER, vec![peer]);
    storage.add_local_folder(BepFolder {
        id: FOLDER.to_owned(),
        label: FOLDER.to_owned(),
        device_ids: vec![peer],
    });
    storage
}

fn wait_for_file(path: &Path, content: &[u8], within: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < within {
        if std::fs::read(path).ok().as_deref() == Some(content) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn test_listener_and_dialer_sync_a_folder_both_ways() {
    let listener_dir = common::fresh_temp_dir("inbound_listener");
    let dialer_dir = common::fresh_temp_dir("inbound_dialer");
    let listener_id = DeviceIdentity::from_seed(&[21u8; 32]).unwrap();
    let dialer_id = DeviceIdentity::from_seed(&[22u8; 32]).unwrap();
    let listener_storage = storage_sharing_with(&listener_dir, dialer_id.device_id);
    let dialer_storage = storage_sharing_with(&dialer_dir, listener_id.device_id);
    std::fs::write(
        dialer_dir.join("data/from_dialer.txt"),
        b"hello from the dialer",
    )
    .unwrap();

    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp_listener.local_addr().unwrap();
    let listener_control = fast_control();
    let dialer_control = fast_control();

    let control = Arc::clone(&listener_control);
    let expected_peer = dialer_id.device_id;
    let accept_thread = thread::spawn(move || {
        let (tcp, peer_addr) = tcp_listener.accept().unwrap();
        let incoming = accept_bep_tls_stream(&listener_id, tcp, TIMEOUT).unwrap();
        assert_eq!(incoming.peer_device_id(), expected_peer);
        let hello = BepHello::new("listener", "test", "1");
        let params = SessionParams {
            identity: &listener_id,
            hello: &hello,
            storage: &listener_storage,
            expected_device_id: Some(expected_peer),
            timeout: TIMEOUT,
            control: Some(control),
        };
        let mut events = Vec::new();
        let result = accept_and_sync(&params, incoming, peer_addr, |e| events.push(e)).unwrap();
        (result, events)
    });

    let control = Arc::clone(&dialer_control);
    let expected_listener = DeviceIdentity::from_seed(&[21u8; 32]).unwrap().device_id;
    let dialer_thread = thread::spawn(move || {
        let hello = BepHello::new("dialer", "test", "1");
        let params = SessionParams {
            identity: &dialer_id,
            hello: &hello,
            storage: &dialer_storage,
            expected_device_id: Some(expected_listener),
            timeout: TIMEOUT,
            control: Some(control),
        };
        connect_and_sync(&params, addr, |_| {}).unwrap()
    });

    // Dialer -> listener: the file present before the session.
    assert!(
        wait_for_file(
            &listener_dir.join("data/from_dialer.txt"),
            b"hello from the dialer",
            Duration::from_secs(10)
        ),
        "the listener did not receive the dialer's file"
    );
    // Listener -> dialer: a file created while the session runs (picked up by a rescan).
    std::fs::write(listener_dir.join("data/from_listener.txt"), b"hi back").unwrap();
    listener_control.request_rescan();
    assert!(
        wait_for_file(
            &dialer_dir.join("data/from_listener.txt"),
            b"hi back",
            Duration::from_secs(10)
        ),
        "the dialer did not receive the listener's file"
    );

    listener_control.request_stop();
    dialer_control.request_stop();
    let (result, events) = accept_thread.join().unwrap();
    let dialer_result = dialer_thread.join().unwrap();
    assert!(matches!(
        result.session_status,
        SyncSessionStatus::Active { .. }
    ));
    assert!(matches!(
        dialer_result.session_status,
        SyncSessionStatus::Active { .. }
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, SyncEvent::PeerConnected { peer_name, .. } if peer_name == "dialer")));
    // Our `Hello` announced a single connection: the dialer opened no secondary connection.
    assert!(events.iter().any(|e| matches!(
        e,
        SyncEvent::SyncCycleCompleted {
            active_connections: 1,
            ..
        }
    )));
}

#[test]
fn test_incoming_session_is_refused_when_the_peer_is_not_the_expected_one() {
    let dir = common::fresh_temp_dir("inbound_unexpected");
    let listener_id = DeviceIdentity::from_seed(&[23u8; 32]).unwrap();
    let dialer_id = DeviceIdentity::from_seed(&[24u8; 32]).unwrap();
    let someone_else = DeviceIdentity::from_seed(&[25u8; 32]).unwrap().device_id;
    let storage = storage_sharing_with(&dir, someone_else);

    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp_listener.local_addr().unwrap();
    let listener_device = listener_id.device_id;
    let dialer = thread::spawn(move || {
        let hello = BepHello::new("dialer", "test", "1");
        let storage = FsSyncStorage::new(std::env::temp_dir().join("vita_syncthing_unused"));
        let params = SessionParams {
            identity: &dialer_id,
            hello: &hello,
            storage: &storage,
            expected_device_id: Some(listener_device),
            timeout: TIMEOUT,
            control: None,
        };
        let _ = connect_and_sync(&params, addr, |_| {});
    });

    let (tcp, peer_addr) = tcp_listener.accept().unwrap();
    let incoming = accept_bep_tls_stream(&listener_id, tcp, TIMEOUT).unwrap();
    let hello = BepHello::new("listener", "test", "1");
    let params = SessionParams {
        identity: &listener_id,
        hello: &hello,
        storage: &storage,
        expected_device_id: Some(someone_else),
        timeout: TIMEOUT,
        control: None,
    };
    let err = accept_and_sync(&params, incoming, peer_addr, |_| {}).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    drop(params);
    dialer.join().unwrap();
}
