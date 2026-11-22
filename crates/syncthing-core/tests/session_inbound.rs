//! Incoming sessions (`tls::accept_bep_tls_stream` + `session::accept_and_sync`): two
//! `syncthing-core` devices, one listening and one dialing, synchronize a folder both ways.

mod common;

use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use syncthing_core::tls::client::open_bep_tls_stream;
use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, connect_and_sync, connect_and_sync_stream,
    read_bep_message, write_bep_message, BepClusterConfig, BepFolder, BepHello, BepIndex,
    BepMessageType, DeviceId, DeviceIdentity, FolderIndexState, FsSyncStorage, SessionParams,
    SyncControl, SyncEvent, SyncSessionStatus, SyncTuning,
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
        ..Default::default()
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
            secondary_inbox: None,
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
            secondary_inbox: None,
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

    // Only the listener stops: it leaves its persistent loop, so its session ran (`Active`).
    // The dialer ends because its peer left: `Active` when the `Close` reached its persistent
    // loop, `ClosedByPeer` when it was still draining its initial cycle (the listener's file
    // can arrive there, within the drain window).
    listener_control.request_stop();
    let (result, events) = accept_thread.join().unwrap();
    assert!(
        matches!(result.session_status, SyncSessionStatus::Active { .. }),
        "listener: {:?}",
        result.session_status
    );
    let dialer_result = dialer_thread.join().unwrap();
    assert!(
        matches!(
            dialer_result.session_status,
            SyncSessionStatus::Active { .. } | SyncSessionStatus::ClosedByPeer(_)
        ),
        "dialer: {:?}",
        dialer_result.session_status
    );
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
            secondary_inbox: None,
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
        secondary_inbox: None,
    };
    let err = accept_and_sync(&params, incoming, peer_addr, |_| {}).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    drop(params);
    dialer.join().unwrap();
}

/// Runs `client` against a fresh listener and returns how long `accept_bep_tls_stream` took to
/// give up on it (it must fail: the client never completes a handshake).
fn handshake_failure_time(
    timeout: Duration,
    client: impl FnOnce(std::net::TcpStream) + Send + 'static,
) -> Duration {
    let identity = DeviceIdentity::from_seed(&[26u8; 32]).unwrap();
    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp_listener.local_addr().unwrap();
    let client = thread::spawn(move || client(std::net::TcpStream::connect(addr).unwrap()));
    let (tcp, _) = tcp_listener.accept().unwrap();
    let start = Instant::now();
    let result = accept_bep_tls_stream(&identity, tcp, timeout);
    let elapsed = start.elapsed();
    assert!(result.is_err(), "the handshake cannot succeed");
    client.join().unwrap();
    elapsed
}

#[test]
fn test_accept_gives_up_on_a_silent_client_after_the_timeout() {
    let timeout = Duration::from_millis(800);
    let elapsed = handshake_failure_time(timeout, |tcp| {
        thread::sleep(Duration::from_secs(3));
        drop(tcp);
    });
    assert!(elapsed >= timeout / 2, "gave up too early: {elapsed:?}");
    assert!(elapsed < timeout * 2, "gave up too late: {elapsed:?}");
}

/// A client sending one byte at a time never lets a single read time out: the timeout must
/// bound the whole handshake, not each read.
#[test]
fn test_accept_gives_up_on_a_trickling_client_after_the_timeout() {
    use std::io::Write;

    let timeout = Duration::from_millis(800);
    let elapsed = handshake_failure_time(timeout, |mut tcp| {
        // A TLS handshake record header announcing 16 KiB, then its body byte by byte.
        let _ = tcp.write_all(&[0x16, 0x03, 0x01, 0x40, 0x00]);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(6) {
            if tcp.write_all(&[0]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    assert!(elapsed < timeout * 2, "gave up too late: {elapsed:?}");
}

/// An incoming session announces a single connection in its `Hello`, whatever the application's
/// `Hello` says: the dialing device then opens no secondary connection to it.
#[test]
fn test_incoming_session_announces_a_single_connection() {
    let dir = common::fresh_temp_dir("inbound_single_connection");
    let listener_id = DeviceIdentity::from_seed(&[26u8; 32]).unwrap();
    let dialer_id = DeviceIdentity::from_seed(&[27u8; 32]).unwrap();
    let listener_device = listener_id.device_id;
    let storage = storage_sharing_with(&dir, dialer_id.device_id);

    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp_listener.local_addr().unwrap();
    let expected_dialer = dialer_id.device_id;
    let listener = thread::spawn(move || {
        let (tcp, peer_addr) = tcp_listener.accept().unwrap();
        let incoming = accept_bep_tls_stream(&listener_id, tcp, TIMEOUT).unwrap();
        // The application would announce three connections; the session must not.
        let mut hello = BepHello::new("listener", "test", "1");
        hello.num_connections = 2;
        let params = SessionParams {
            identity: &listener_id,
            hello: &hello,
            storage: &storage,
            expected_device_id: Some(expected_dialer),
            timeout: TIMEOUT,
            control: None,
            secondary_inbox: None,
        };
        let _ = accept_and_sync(&params, incoming, peer_addr, |_| {});
    });

    let (mut stream, _) = open_bep_tls_stream(&dialer_id, addr, Some(listener_device), TIMEOUT)
        .expect("dial the listener");
    let peer_hello = BepHello::new("dialer", "test", "1")
        .exchange(&mut stream)
        .expect("Hello exchange");
    assert_eq!(peer_hello.num_connections, 1);
    drop(stream);
    listener.join().unwrap();
}

#[test]
fn test_inbound_accepts_immediate_peer_cluster_config_with_single_reply() {
    let listener_dir = common::fresh_temp_dir("inbound_immediate_cfg_srv");
    let listener_id = DeviceIdentity::from_seed(&[28u8; 32]).unwrap();
    let dialer_id = DeviceIdentity::from_seed(&[29u8; 32]).unwrap();

    // Server storage ONLY has `set_folder_devices` (no `add_local_folder`, matching `vita-syncthing-app`).
    let data = listener_dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("vita.txt"), b"from vita").unwrap();
    let mut listener_storage = FsSyncStorage::new(listener_dir.join("db"));
    listener_storage.set_folder_path(FOLDER, &data);
    listener_storage.set_folder_devices(FOLDER, vec![dialer_id.device_id]);

    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp_listener.local_addr().unwrap();
    let expected_dialer = dialer_id.device_id;
    let accept_thread = thread::spawn(move || {
        let (tcp, peer_addr) = tcp_listener.accept().unwrap();
        let incoming = accept_bep_tls_stream(&listener_id, tcp, TIMEOUT).unwrap();
        let hello = BepHello::new("vita-listener", "test", "1");
        let params = SessionParams {
            identity: &listener_id,
            hello: &hello,
            storage: &listener_storage,
            expected_device_id: Some(expected_dialer),
            timeout: TIMEOUT,
            control: None,
            secondary_inbox: None,
        };
        accept_and_sync(&params, incoming, peer_addr, |_| {}).unwrap()
    });

    let listener_dev = DeviceIdentity::from_seed(&[28u8; 32]).unwrap().device_id;
    let (mut stream, _) =
        open_bep_tls_stream(&dialer_id, addr, Some(listener_dev), TIMEOUT).unwrap();
    let _ = BepHello::new("syncthing-dialer", "syncthing", "v2.0.0")
        .exchange(&mut stream)
        .unwrap();

    // Immediately send ClusterConfig (like a real Syncthing server dialing the PS Vita).
    let cfg_payload = BepClusterConfig::encode_for_peers(
        &[(
            BepFolder {
                id: FOLDER.to_owned(),
                label: FOLDER.to_owned(),
                device_ids: vec![dialer_id.device_id, listener_dev],
                ..Default::default()
            },
            FolderIndexState::default(),
        )],
        dialer_id.device_id,
        "syncthing-dialer",
        listener_dev,
    );
    write_bep_message(&mut stream, BepMessageType::ClusterConfig, &cfg_payload).unwrap();

    // Server must reply with exactly one ClusterConfig containing `FOLDER` (not empty).
    let (msg_type, payload) = read_bep_message(&mut stream).unwrap();
    assert_eq!(msg_type, BepMessageType::ClusterConfig);
    let reply = BepClusterConfig::decode(&payload).unwrap();
    let reply_ids: Vec<&str> = reply.folders.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(reply_ids, vec![FOLDER]);

    // Send an empty Index and drain messages until the server's Index arrives; ensure no second
    // ClusterConfig is ever sent.
    let empty_index = BepIndex {
        folder: FOLDER.to_owned(),
        files: Vec::new(),
    };
    write_bep_message(&mut stream, BepMessageType::Index, &empty_index.encode()).unwrap();

    let mut cluster_configs_seen = 1usize;
    let mut index_seen = false;
    stream.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
    while !index_seen {
        match read_bep_message(&mut stream) {
            Ok((BepMessageType::ClusterConfig, _)) => cluster_configs_seen += 1,
            Ok((BepMessageType::Index, idx_payload)) => {
                let idx = BepIndex::decode(&idx_payload).unwrap();
                assert_eq!(idx.folder, FOLDER);
                index_seen = true;
            }
            Ok(_) => {}
            Err(e) => panic!("unexpected read error before Index: {e}"),
        }
    }
    assert_eq!(
        cluster_configs_seen, 1,
        "must never send a second ClusterConfig"
    );
    drop(stream);

    let result = accept_thread.join().unwrap();
    assert!(
        matches!(result.session_status, SyncSessionStatus::Active { .. }),
        "expected Active session, got {:?}",
        result.session_status
    );
}

#[test]
fn test_inbound_server_fallback_announces_configured_folder_and_label() {
    let listener_dir = common::fresh_temp_dir("inbound_fallback_srv");
    let dialer_dir = common::fresh_temp_dir("inbound_fallback_cli");
    let listener_id = DeviceIdentity::from_seed(&[30u8; 32]).unwrap();
    let dialer_id = DeviceIdentity::from_seed(&[31u8; 32]).unwrap();

    // Server storage ONLY has `set_folder_devices` + `set_folder_label` (no `add_local_folder`).
    let srv_data = listener_dir.join("data");
    std::fs::create_dir_all(&srv_data).unwrap();
    std::fs::write(srv_data.join("from_srv.txt"), b"hello from server").unwrap();
    let mut listener_storage = FsSyncStorage::new(listener_dir.join("db"));
    listener_storage.set_folder_path(FOLDER, &srv_data);
    listener_storage.set_folder_devices(FOLDER, vec![dialer_id.device_id]);
    listener_storage.set_folder_label(FOLDER, "Vita Saves");

    let cli_data = dialer_dir.join("data");
    std::fs::create_dir_all(&cli_data).unwrap();
    std::fs::write(cli_data.join("from_cli.txt"), b"hello from dialer").unwrap();
    let mut dialer_storage = FsSyncStorage::new(dialer_dir.join("db"));
    dialer_storage.set_folder_path(FOLDER, &cli_data);
    dialer_storage.set_folder_devices(FOLDER, vec![listener_id.device_id]);

    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp_listener.local_addr().unwrap();
    let expected_dialer = dialer_id.device_id;
    let listener_control = fast_control();
    let srv_ctrl = Arc::clone(&listener_control);
    let accept_thread = thread::spawn(move || {
        let (tcp, peer_addr) = tcp_listener.accept().unwrap();
        let incoming = accept_bep_tls_stream(&listener_id, tcp, TIMEOUT).unwrap();
        let hello = BepHello::new("vita-listener", "test", "1");
        let params = SessionParams {
            identity: &listener_id,
            hello: &hello,
            storage: &listener_storage,
            expected_device_id: Some(expected_dialer),
            timeout: TIMEOUT,
            control: Some(srv_ctrl),
            secondary_inbox: None,
        };
        accept_and_sync(&params, incoming, peer_addr, |_| {}).unwrap()
    });

    let expected_listener = DeviceIdentity::from_seed(&[30u8; 32]).unwrap().device_id;
    let dialer_control = fast_control();
    let cli_ctrl = Arc::clone(&dialer_control);
    let dialer_thread = thread::spawn(move || {
        let hello = BepHello::new("syncthing-core-dialer", "test", "1");
        let params = SessionParams {
            identity: &dialer_id,
            hello: &hello,
            storage: &dialer_storage,
            expected_device_id: Some(expected_listener),
            timeout: TIMEOUT,
            control: Some(cli_ctrl),
            secondary_inbox: None,
        };
        let mut events = Vec::new();
        let res = connect_and_sync(&params, addr, |e| events.push(e)).unwrap();
        (res, events)
    });

    assert!(
        wait_for_file(
            &srv_data.join("from_cli.txt"),
            b"hello from dialer",
            Duration::from_secs(10)
        ),
        "server did not receive dialer's file"
    );
    assert!(
        wait_for_file(
            &cli_data.join("from_srv.txt"),
            b"hello from server",
            Duration::from_secs(10)
        ),
        "dialer did not receive server's file"
    );

    listener_control.request_stop();
    let srv_res = accept_thread.join().unwrap();
    assert!(matches!(
        srv_res.session_status,
        SyncSessionStatus::Active { .. }
    ));
    let (_, dialer_events) = dialer_thread.join().unwrap();

    let cfgs: Vec<&Vec<BepFolder>> = dialer_events
        .iter()
        .filter_map(|e| match e {
            SyncEvent::ClusterConfigReceived { folders, .. } => Some(folders),
            _ => None,
        })
        .collect();
    assert_eq!(cfgs.len(), 1, "server must send exactly 1 ClusterConfig");
    assert_eq!(cfgs[0].len(), 1);
    assert_eq!(cfgs[0][0].id, FOLDER);
    assert_eq!(cfgs[0][0].label, "Vita Saves");
}

#[test]
fn test_connect_and_sync_stream_announces_single_connection_in_hello() {
    let listener_id = DeviceIdentity::from_seed(&[32u8; 32]).unwrap();
    let dialer_id = DeviceIdentity::from_seed(&[33u8; 32]).unwrap();
    let tcp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp_listener.local_addr().unwrap();

    let listener = thread::spawn(move || {
        let (tcp, _) = tcp_listener.accept().unwrap();
        let incoming = accept_bep_tls_stream(&listener_id, tcp, TIMEOUT).unwrap();
        let mut stream = incoming.into_stream();
        let peer_hello = BepHello::new("relay-peer", "test", "1")
            .exchange(&mut stream)
            .unwrap();
        drop(stream);
        peer_hello.num_connections
    });

    let tcp = std::net::TcpStream::connect(addr).unwrap();
    let storage = FsSyncStorage::new(common::fresh_temp_dir("stream_hello_single"));
    let mut hello = BepHello::new("vita", "test", "1");
    hello.num_connections = 3;
    let params = SessionParams {
        identity: &dialer_id,
        hello: &hello,
        storage: &storage,
        expected_device_id: Some(DeviceIdentity::from_seed(&[32u8; 32]).unwrap().device_id),
        timeout: TIMEOUT,
        control: None,
        secondary_inbox: None,
    };
    let res = connect_and_sync_stream(&params, tcp, addr, |_| {}).unwrap();
    assert!(matches!(
        res.session_status,
        SyncSessionStatus::AwaitingPairing
    ));
    assert_eq!(listener.join().unwrap(), 1);
}
