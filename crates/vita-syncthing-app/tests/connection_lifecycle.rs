//! End-to-end host integration tests for the full connection status lifecycle:
//!
//! 1. Offline / pre-connection state before any peer is reachable
//! 2. Connecting & initial sync (`PeerConnected` -> `SyncActivity::Downloading`)
//! 3. Connected & synchronized (`SyncCycleCompleted` -> `SyncActivity::Ready`, files synced both ways)
//! 4. Live incremental sync while staying connected (no spurious disconnect)
//! 5. Peer disconnection (BEP `Close`) -> clean post-disconnection state (`Offline` / `Idle` / `WaitingForLan`)
//! 6. Automatic reconnection & sync once the peer returns
//! 7. Early disconnection mid-initial-sync (before `SyncCycleCompleted`) -> post-disconnection state still resets cleanly

mod common;

use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use common::temp_test_dir;
use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, BepFolder, BepHello, DeviceIdentity, FsSyncStorage,
    SessionParams, SyncControl, SyncEvent, SyncTuning,
};
use vita_syncthing_app::application::{SyncCoordinator, UploadActivity};
use vita_syncthing_app::domain::defaults::UPLOAD_DISPLAY_GRACE;
use vita_syncthing_app::domain::{
    AppState, KnownDevice, ShareConfigEntry, ShareMode, SyncActivity, UiText,
};
use vita_syncthing_app::infrastructure::{spawn_sync_worker, ConfigRepository, WorkerMsg};

const FOLDER: &str = "lifecycle-share";
const TIMEOUT: Duration = Duration::from_secs(15);

fn peer_identity() -> DeviceIdentity {
    DeviceIdentity::from_seed(&[0x51; 32]).unwrap()
}

fn vita_identity() -> DeviceIdentity {
    DeviceIdentity::from_seed(&[0x52; 32]).unwrap()
}

fn fast_control() -> Arc<SyncControl> {
    SyncControl::with_tuning(SyncTuning {
        rescan_interval: Duration::from_millis(250),
        keepalive_interval: Duration::from_secs(20),
    })
}

fn make_peer_storage(root: &Path, vita_id: syncthing_core::DeviceId) -> FsSyncStorage {
    let data = root.join("share");
    fs::create_dir_all(&data).unwrap();
    let mut storage = FsSyncStorage::new(root.join("db"));
    storage.set_folder_path(FOLDER, &data);
    storage.set_folder_devices(FOLDER, vec![vita_id]);
    storage.add_local_folder(BepFolder {
        id: FOLDER.to_owned(),
        label: "Lifecycle Share".to_owned(),
        device_ids: vec![vita_id],
        ..Default::default()
    });
    storage
}

/// Spawns a real BEP server on `listener` that accepts one connection from `expected_vita`
/// and runs `accept_and_sync` until `peer_control.request_stop()` is called.
fn spawn_beps_server(
    listener: TcpListener,
    server_id: DeviceIdentity,
    storage: FsSyncStorage,
    expected_vita: syncthing_core::DeviceId,
    peer_control: Arc<SyncControl>,
) -> thread::JoinHandle<(TcpListener, Vec<SyncEvent>)> {
    thread::spawn(move || {
        let mut events = Vec::new();
        let (tcp, peer_addr) = listener.accept().expect("accept vita");
        let incoming =
            accept_bep_tls_stream(&server_id, tcp, Duration::from_secs(5)).expect("server TLS");
        assert_eq!(incoming.peer_device_id(), expected_vita);
        let hello = BepHello::new("test-server", "syncthing-cli", "0.1.0");
        let params = SessionParams {
            identity: &server_id,
            hello: &hello,
            storage: &storage,
            expected_device_id: Some(expected_vita),
            timeout: Duration::from_secs(5),
            control: Some(peer_control),
            secondary_inbox: None,
        };
        let _ = accept_and_sync(&params, incoming, peer_addr, |ev| events.push(ev));
        (listener, events)
    })
}

fn wait_until(
    state: &mut AppState,
    repo: &ConfigRepository,
    rx: &mut Option<Receiver<WorkerMsg>>,
    timeout: Duration,
    mut pred: impl FnMut(&AppState) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        SyncCoordinator::process_worker_messages(state, repo, rx);
        if pred(state) {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    SyncCoordinator::process_worker_messages(state, repo, rx);
    pred(state)
}

#[test]
fn full_connection_lifecycle_sync_disconnect_post_disconnect_and_reconnect() {
    let vita_dir = temp_test_dir("lifecycle-vita");
    let peer_dir = temp_test_dir("lifecycle-peer");
    let vita_share = Path::new(&vita_dir).join("share");
    let peer_share = Path::new(&peer_dir).join("share");
    fs::create_dir_all(&vita_share).unwrap();
    fs::create_dir_all(&peer_share).unwrap();

    fs::write(vita_share.join("from_vita.txt"), b"vita payload").unwrap();
    fs::write(peer_share.join("from_server.txt"), b"server payload").unwrap();

    let vita_id = vita_identity();
    let server_id = peer_identity();
    let peer_storage = make_peer_storage(Path::new(&peer_dir), vita_id.device_id);

    // Bind a local TCP listener for the peer, but do NOT accept yet until we verify initial state.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_addr = listener.local_addr().unwrap();

    let repo = ConfigRepository::new(&vita_dir);
    let peer = KnownDevice {
        device_id: server_id.device_id,
        name: "TestServer".to_owned(),
        address: server_addr.to_string(),
    };
    let share = ShareConfigEntry {
        folder_id: FOLDER.to_owned(),
        label: "Lifecycle Share".to_owned(),
        target_path: vita_share.to_string_lossy().into_owned(),
        enabled: true,
        mode: ShareMode::ReadWrite,
        shared_devices: vec![server_id.device_id],
    };
    let mut state = AppState::new(vec![share.clone()], vec![peer.clone()], &vita_dir);

    // Phase 1: Pre-connection (Offline / Idle).
    assert!(!state.session_connected);
    assert!(!state.sync_in_progress);
    assert!(!state.connected_peers.is_active());
    assert!(!state.initial_sync.is_active());
    assert_eq!(state.sync_activity(), SyncActivity::Idle);

    // Phase 2 & 3: Start peer server + Vita worker -> connect and sync both ways.
    let peer_ctl_1 = fast_control();
    let server_handle_1 = spawn_beps_server(
        listener,
        server_id.clone(),
        peer_storage.clone(),
        vita_id.device_id,
        Arc::clone(&peer_ctl_1),
    );

    let (rx, vita_ctl) = spawn_sync_worker(
        &vita_dir,
        vita_id.clone(),
        vec![share],
        vec![peer.clone()],
        vec![peer],
        Vec::new(),
    );
    let mut worker_rx = Some(rx);

    assert!(
        wait_until(&mut state, &repo, &mut worker_rx, TIMEOUT, |s| {
            s.session_connected
                && !s.sync_in_progress
                && fs::read(vita_share.join("from_server.txt")).ok().as_deref()
                    == Some(b"server payload".as_slice())
                && fs::read(peer_share.join("from_vita.txt")).ok().as_deref()
                    == Some(b"vita payload".as_slice())
        }),
        "Vita did not reach connected+synced state; status={:?}, logs={:?}",
        state.status_line,
        state.logs
    );

    // Connected & synchronized status assertions.
    // Because we uploaded `from_vita.txt`, `uploading` stays true until `UPLOAD_DISPLAY_GRACE`
    // elapses (handled by `UploadActivity` in the main loop).
    let mut uploads = UploadActivity::new();
    let now = Instant::now();
    uploads.update(&mut state, now);
    assert_eq!(state.sync_activity(), SyncActivity::Uploading);
    uploads.update(
        &mut state,
        now + UPLOAD_DISPLAY_GRACE + Duration::from_millis(1),
    );

    assert!(state.session_connected);
    assert!(state.connected_peers.is_active());
    assert!(!state.initial_sync.is_active());
    assert!(!state.sync_in_progress);
    assert!(!state.uploading);
    assert_eq!(state.sync_activity(), SyncActivity::Ready);
    assert_eq!(state.progress_pct, Some(100));
    assert!(
        matches!(state.status_line, UiText::SyncSummary { .. }),
        "expected SyncSummary, got {:?}",
        state.status_line
    );
    assert!(
        !state.logs.iter().any(|l| l.contains("ended (")),
        "no disconnect should occur while peer is online: {:?}",
        state.logs
    );

    // Phase 4: Live incremental sync while staying stably connected.
    fs::write(peer_share.join("live_update.txt"), b"live delta").unwrap();
    peer_ctl_1.request_rescan();
    assert!(
        wait_until(&mut state, &repo, &mut worker_rx, TIMEOUT, |_| {
            fs::read(vita_share.join("live_update.txt")).ok().as_deref()
                == Some(b"live delta".as_slice())
        }),
        "live_update.txt did not sync while connected; logs={:?}",
        state.logs
    );
    assert!(state.session_connected);
    assert_eq!(state.sync_activity(), SyncActivity::Ready);
    assert!(
        !state.logs.iter().any(|l| l.contains("ended (")),
        "live update must not drop the session: {:?}",
        state.logs
    );

    // Phase 5: Peer disconnects cleanly (BEP `Close`) -> verify post-disconnection state.
    peer_ctl_1.request_stop();
    let (listener, server_events_1) = server_handle_1.join().expect("server 1 join");
    assert!(
        server_events_1
            .iter()
            .any(|e| matches!(e, SyncEvent::PeerConnected { .. })),
        "server should have recorded PeerConnected"
    );
    assert!(
        server_events_1
            .iter()
            .any(|e| matches!(e, SyncEvent::SyncCycleCompleted { .. })),
        "server should have recorded SyncCycleCompleted"
    );
    assert!(
        server_events_1
            .iter()
            .any(|e| matches!(e, SyncEvent::SessionEnded { .. })),
        "server should have recorded SessionEnded on stop"
    );

    assert!(
        wait_until(&mut state, &repo, &mut worker_rx, TIMEOUT, |s| {
            !s.session_connected
        }),
        "Vita did not transition to disconnected state after peer stop; logs={:?}",
        state.logs
    );

    // Strict post-disconnection assertions on Vita AppState.
    assert!(!state.session_connected);
    assert!(!state.connected_peers.is_active());
    assert!(!state.initial_sync.is_active());
    assert!(!state.sync_in_progress);
    assert!(!state.uploading);
    assert_eq!(state.sync_activity(), SyncActivity::Idle);
    assert_eq!(state.status_line, UiText::WaitingForLan);
    assert_eq!(state.progress_pct, Some(0));
    assert_eq!(state.current_file, UiText::raw("-"));
    assert!(
        state
            .logs
            .iter()
            .any(|l| l.contains("ended (closed by peer: closing). Reconnecting...")),
        "expected clean close log line, got {:?}",
        state.logs
    );

    // Phase 6: Peer comes back online -> Vita reconnects automatically and syncs new file.
    fs::write(peer_share.join("post_reconnect.txt"), b"after reconnect").unwrap();
    let peer_ctl_2 = fast_control();
    let server_handle_2 = spawn_beps_server(
        listener,
        server_id,
        peer_storage,
        vita_id.device_id,
        Arc::clone(&peer_ctl_2),
    );

    assert!(
        wait_until(&mut state, &repo, &mut worker_rx, TIMEOUT, |s| {
            s.session_connected
                && !s.sync_in_progress
                && fs::read(vita_share.join("post_reconnect.txt"))
                    .ok()
                    .as_deref()
                    == Some(b"after reconnect".as_slice())
        }),
        "Vita did not reconnect and sync post_reconnect.txt; status={:?}, logs={:?}",
        state.status_line,
        state.logs
    );
    assert_eq!(state.sync_activity(), SyncActivity::Ready);

    // Clean Vita shutdown -> server observes clean BEP `Close` disconnect.
    vita_ctl.request_stop();
    let (_, server_events_2) = server_handle_2.join().expect("server 2 join");
    assert!(
        server_events_2.iter().any(|e| matches!(
            e,
            SyncEvent::SessionEnded { reason, .. } if reason.contains("closed by peer: closing")
        )),
        "server should see clean BEP Close on Vita shutdown: {server_events_2:?}"
    );

    let _ = fs::remove_dir_all(&vita_dir);
    let _ = fs::remove_dir_all(&peer_dir);
}

#[test]
fn disconnect_during_initial_sync_resets_post_disconnect_state_cleanly() {
    let dir = temp_test_dir("lifecycle-early-disconnect");
    let repo = ConfigRepository::new(&dir);
    let peer_id = peer_identity().device_id;
    let peer = KnownDevice {
        device_id: peer_id,
        name: "Server".to_owned(),
        address: "127.0.0.1:22000".to_owned(),
    };
    let share = ShareConfigEntry {
        folder_id: FOLDER.to_owned(),
        label: "Share".to_owned(),
        target_path: dir.clone(),
        enabled: true,
        mode: ShareMode::ReadWrite,
        shared_devices: vec![peer_id],
    };
    let mut state = AppState::new(vec![share], vec![peer], &dir);

    let feed = |state: &mut AppState, ev: SyncEvent| {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(WorkerMsg::SyncEvent(ev)).unwrap();
        drop(tx);
        let mut opt = Some(rx);
        SyncCoordinator::process_worker_messages(state, &repo, &mut opt);
    };

    // Peer connects and starts downloading (still in initial_sync, before SyncCycleCompleted).
    feed(
        &mut state,
        SyncEvent::PeerConnected {
            peer_addr: "127.0.0.1:22000".parse().unwrap(),
            peer_device_id: peer_id,
            peer_name: "Server".to_owned(),
            client_version: "v2.0".to_owned(),
        },
    );
    assert!(state.initial_sync.is_active());
    assert!(state.sync_in_progress);
    assert_eq!(state.sync_activity(), SyncActivity::Downloading);
    assert_eq!(state.status_line, UiText::ExchangingIndex);

    // Peer disconnects mid-initial-sync -> post-disconnection state must NOT stay stuck on Syncing.
    feed(
        &mut state,
        SyncEvent::SessionEnded {
            peer_device_id: peer_id,
            reason: "closed by peer: closing".to_owned(),
        },
    );
    assert!(!state.session_connected);
    assert!(!state.connected_peers.is_active());
    assert!(!state.initial_sync.is_active());
    assert!(!state.sync_in_progress);
    assert!(!state.uploading);
    assert_eq!(state.sync_activity(), SyncActivity::Idle);
    assert_eq!(state.status_line, UiText::WaitingForLan);
    assert_eq!(state.progress_pct, Some(0));
    assert_eq!(state.current_file, UiText::raw("-"));

    let _ = fs::remove_dir_all(&dir);
}
