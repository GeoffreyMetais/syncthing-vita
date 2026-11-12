mod common;

use std::sync::mpsc;

use syncthing_core::{DeviceId, SyncEvent};
use vita_syncthing_app::application::SyncCoordinator;
use vita_syncthing_app::domain::{AppState, UiText};
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
