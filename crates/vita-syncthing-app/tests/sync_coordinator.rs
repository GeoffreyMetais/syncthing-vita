mod common;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use syncthing_core::{DeviceId, SyncEvent};
use vita_syncthing_app::application::upload_activity::UPLOAD_DISPLAY_GRACE;
use vita_syncthing_app::application::{SyncCoordinator, UploadActivity};
use vita_syncthing_app::domain::{AppState, SyncActivity, UiText};
use vita_syncthing_app::infrastructure::{ConfigRepository, WorkerMsg};

#[test]
fn test_indeterminate_init_progress_until_bytes_received() {
    let repo = ConfigRepository::new(&common::temp_test_dir("sync-coordinator-progress"));
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");
    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);
    let peer = DeviceId([9; 32]);

    // Before sync starts, progress is 0%.
    assert_eq!(state.progress_pct, Some(0));

    // Connecting transitions to an indeterminate init step (`None`), not a fake percentage.
    tx.send(WorkerMsg::Progress {
        pct: None,
        status: UiText::ConnectingTo {
            name: "NAS".to_owned(),
            addr: "192.168.1.67:22000".to_owned(),
        },
        file: UiText::raw("-"),
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.progress_pct, None);

    // Peer 1 connects: still in indeterminate init / index exchange until bytes arrive.
    tx.send(WorkerMsg::SyncEvent(SyncEvent::PeerConnected {
        peer_addr: "192.168.1.67:22000".parse().unwrap(),
        peer_device_id: peer,
        peer_name: "NAS".to_owned(),
        client_version: "v2.1.7".to_owned(),
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.progress_pct, None);
    assert_eq!(state.status_line, UiText::ExchangingIndex);

    // Actual bytes received: progress reports real byte percentage (25% of 1000 bytes).
    tx.send(WorkerMsg::SyncEvent(SyncEvent::BlockDownloaded {
        folder: "vita-savedata".to_owned(),
        name: "save.bin".to_owned(),
        offset: 0,
        size: 250,
        total_size: 1000,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.progress_pct, Some(25));

    // Offline Peer 2 retrying in the background must not reset progress or status.
    tx.send(WorkerMsg::Progress {
        pct: None,
        status: UiText::ConnectingTo {
            name: "Offline Mac".to_owned(),
            addr: "192.168.1.39:22000".to_owned(),
        },
        file: UiText::raw("-"),
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.progress_pct, Some(25));

    // Peer 1 finishes its initial sync cycle (100%, session_connected = true).
    tx.send(WorkerMsg::SyncEvent(SyncEvent::SyncCycleCompleted {
        peer_device_id: peer,
        active_connections: 3,
        files_up_to_date: 1,
        files_downloaded: 1,
        bytes_downloaded: 1000,
        blocks_uploaded: 0,
        bytes_uploaded: 0,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.progress_pct, Some(100));
    assert!(state.session_connected);

    // Offline Peer 2 retrying after sync completion must not regress progress or status.
    tx.send(WorkerMsg::Progress {
        pct: None,
        status: UiText::ConnectingTo {
            name: "Offline Mac".to_owned(),
            addr: "192.168.1.39:22000".to_owned(),
        },
        file: UiText::raw("-"),
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.progress_pct, Some(100));
    assert!(matches!(state.status_line, UiText::SyncSummary { .. }));
}

#[test]
fn test_sidebar_ready_only_once_the_session_is_connected_and_idle() {
    let repo = ConfigRepository::new(&common::temp_test_dir("sync-coordinator-ready"));
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");
    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);
    let peer = DeviceId([7; 32]);

    state.prepare_manual_sync();
    assert!(!state.is_sync_ready());

    tx.send(WorkerMsg::SyncEvent(SyncEvent::SyncCycleCompleted {
        peer_device_id: peer,
        active_connections: 1,
        files_up_to_date: 1,
        files_downloaded: 0,
        bytes_downloaded: 0,
        blocks_uploaded: 0,
        bytes_uploaded: 0,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(state.is_sync_ready());

    // A new transfer leaves the Ready state.
    tx.send(WorkerMsg::SyncEvent(SyncEvent::BlockDownloaded {
        folder: "vita-savedata".to_owned(),
        name: "save.bin".to_owned(),
        offset: 0,
        size: 10,
        total_size: 100,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.is_sync_ready());

    // The worker ending (session dropped) shows the last session again, not Ready.
    tx.send(WorkerMsg::Finished {
        summary: UiText::SyncFinished,
        allow_auto_reconnect: true,
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.is_sync_ready());
}

#[test]
fn test_serving_blocks_in_a_ready_session_is_shown_as_uploading_then_ready() {
    let repo = ConfigRepository::new(&common::temp_test_dir("sync-coordinator-uploading"));
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");
    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);
    let mut uploads = UploadActivity::new();
    let start = Instant::now();

    tx.send(WorkerMsg::SyncEvent(SyncEvent::SyncCycleCompleted {
        peer_device_id: DeviceId([5; 32]),
        active_connections: 1,
        files_up_to_date: 1,
        files_downloaded: 0,
        bytes_downloaded: 0,
        blocks_uploaded: 0,
        bytes_uploaded: 0,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.sync_activity(), SyncActivity::Ready);

    // The peer requests a block: uploading, never "idle" with a stale 100%.
    tx.send(WorkerMsg::SyncEvent(SyncEvent::BlockUploaded {
        folder: "vita-savedata".to_owned(),
        name: "save.bin".to_owned(),
        offset: 0,
        size: 10,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!uploads.update(&mut state, start));
    assert_eq!(state.sync_activity(), SyncActivity::Uploading);

    // Still uploading within the grace period...
    assert!(!uploads.update(&mut state, start + UPLOAD_DISPLAY_GRACE));
    assert_eq!(state.sync_activity(), SyncActivity::Uploading);

    // ...then back to Ready once the peer stopped requesting blocks.
    let later = start + UPLOAD_DISPLAY_GRACE + Duration::from_secs(1);
    assert!(uploads.update(&mut state, later));
    assert_eq!(state.sync_activity(), SyncActivity::Ready);
    assert!(!uploads.update(&mut state, later));

    // A download takes precedence over serving blocks.
    for event in [
        SyncEvent::BlockUploaded {
            folder: "vita-savedata".to_owned(),
            name: "a.bin".to_owned(),
            offset: 0,
            size: 10,
        },
        SyncEvent::BlockDownloaded {
            folder: "vita-savedata".to_owned(),
            name: "b.bin".to_owned(),
            offset: 0,
            size: 10,
            total_size: 100,
        },
    ] {
        tx.send(WorkerMsg::SyncEvent(event)).unwrap();
    }
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.sync_activity(), SyncActivity::Downloading);

    // The worker ending clears every activity.
    tx.send(WorkerMsg::Finished {
        summary: UiText::SyncFinished,
        allow_auto_reconnect: true,
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.sync_activity(), SyncActivity::Idle);
}

#[test]
fn test_off_lan_configured_peers_do_not_stay_stuck_syncing() {
    use vita_syncthing_app::domain::KnownDevice;

    let repo = ConfigRepository::new(&common::temp_test_dir("sync-coordinator-off-lan"));
    let peer = DeviceId([3; 32]);
    let mut state = AppState::new(
        Vec::new(),
        vec![KnownDevice {
            device_id: peer,
            name: "NAS".to_owned(),
            address: "192.168.1.67:22000".to_owned(),
        }],
        "ux0:",
    );
    assert!(state.has_configured_peers());

    // Manual sync / worker start with configured peers but no live session stays Idle until a peer connects.
    state.prepare_manual_sync();
    assert!(!state.sync_in_progress);
    assert_eq!(state.sync_activity(), SyncActivity::Idle);

    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);
    tx.send(WorkerMsg::Progress {
        pct: None,
        status: UiText::ConnectingTo {
            name: "NAS".to_owned(),
            addr: "192.168.1.67:22000".to_owned(),
        },
        file: UiText::raw("-"),
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.sync_in_progress);
    assert_eq!(state.sync_activity(), SyncActivity::Idle);

    // Once a peer actually connects, sync_in_progress becomes true; if it drops mid-sync, it clears.
    tx.send(WorkerMsg::SyncEvent(SyncEvent::PeerConnected {
        peer_addr: "192.168.1.67:22000".parse().unwrap(),
        peer_device_id: peer,
        peer_name: "NAS".to_owned(),
        client_version: "v2.1.7".to_owned(),
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(state.sync_in_progress);
    assert_eq!(state.sync_activity(), SyncActivity::Downloading);

    tx.send(WorkerMsg::SyncEvent(SyncEvent::SessionEnded {
        peer_device_id: peer,
        reason: "connection reset".to_owned(),
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.sync_in_progress);
    assert_eq!(state.sync_activity(), SyncActivity::Idle);
}

#[test]
fn test_multi_peer_sync_in_progress_clears_only_when_all_peers_finish() {
    let repo = ConfigRepository::new(&common::temp_test_dir("sync-coordinator-multi-peer"));
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");
    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);
    let peer_a = DeviceId([1; 32]);
    let peer_b = DeviceId([2; 32]);

    for (peer, name) in [(peer_a, "Desktop"), (peer_b, "NAS")] {
        tx.send(WorkerMsg::SyncEvent(SyncEvent::PeerConnected {
            peer_addr: "192.168.1.10:22000".parse().unwrap(),
            peer_device_id: peer,
            peer_name: name.to_owned(),
            client_version: "v2.1.7".to_owned(),
        }))
        .unwrap();
    }
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(state.sync_in_progress);

    tx.send(WorkerMsg::SyncEvent(SyncEvent::SyncCycleCompleted {
        peer_device_id: peer_a,
        active_connections: 2,
        files_up_to_date: 1,
        files_downloaded: 0,
        bytes_downloaded: 0,
        blocks_uploaded: 0,
        bytes_uploaded: 0,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(state.sync_in_progress);

    tx.send(WorkerMsg::SyncEvent(SyncEvent::SyncCycleCompleted {
        peer_device_id: peer_b,
        active_connections: 2,
        files_up_to_date: 1,
        files_downloaded: 0,
        bytes_downloaded: 0,
        blocks_uploaded: 0,
        bytes_uploaded: 0,
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.sync_in_progress);
    assert_eq!(state.sync_activity(), SyncActivity::Ready);
}

fn cycle_completed(peer: DeviceId) -> WorkerMsg {
    WorkerMsg::SyncEvent(SyncEvent::SyncCycleCompleted {
        peer_device_id: peer,
        active_connections: 1,
        files_up_to_date: 0,
        files_downloaded: 0,
        bytes_downloaded: 0,
        blocks_uploaded: 0,
        bytes_uploaded: 0,
    })
}

fn session_ended(peer: DeviceId) -> WorkerMsg {
    WorkerMsg::SyncEvent(SyncEvent::SessionEnded {
        peer_device_id: peer,
        reason: "primary connection lost".to_owned(),
    })
}

/// The app is "Ready" only while at least one peer session is established: once the last one
/// ended it is offline again, and a reconnecting peer's status is displayed.
#[test]
fn test_session_connected_follows_the_connected_peers() {
    let repo = ConfigRepository::new(&common::temp_test_dir("sync-coordinator-disconnect"));
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");
    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);
    let (nas, mac) = (DeviceId([1; 32]), DeviceId([2; 32]));

    tx.send(cycle_completed(nas)).unwrap();
    tx.send(cycle_completed(mac)).unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.sync_activity(), SyncActivity::Ready);

    // One of two peers leaves: still connected.
    tx.send(session_ended(nas)).unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(state.session_connected);
    assert!(matches!(state.status_line, UiText::SyncSummary { .. }));

    // The last one leaves: offline, not "Ready".
    tx.send(session_ended(mac)).unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.session_connected);
    assert_eq!(state.sync_activity(), SyncActivity::Idle);
    assert_eq!(state.status_line, UiText::WaitingForLan);

    // The reconnection attempt is shown.
    let connecting = UiText::ConnectingTo {
        name: "Mac".to_owned(),
        addr: "192.168.1.39:22000".to_owned(),
    };
    tx.send(WorkerMsg::Progress {
        pct: None,
        status: connecting.clone(),
        file: UiText::raw("-"),
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert_eq!(state.status_line, connecting);
}
