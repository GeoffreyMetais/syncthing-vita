//! `SyncCoordinator`: every `WorkerMsg` / `SyncEvent` maps to the expected `AppState` fields,
//! toasts and protocol log lines, and progress updates never regress while idle.

mod common;

use std::net::SocketAddr;
use std::sync::mpsc;

use common::temp_test_dir;
use syncthing_core::{BepFolder, DeviceId, SyncEvent};
use vita_syncthing_app::application::SyncCoordinator;
use vita_syncthing_app::domain::{AppState, UiText};
use vita_syncthing_app::infrastructure::{ConfigRepository, WorkerMsg};

/// Delivers one message through a channel, as the worker thread does, and returns the
/// coordinator's "redraw" result.
fn apply(state: &mut AppState, repo: &ConfigRepository, msg: WorkerMsg) -> bool {
    let (tx, rx) = mpsc::channel();
    tx.send(msg).unwrap();
    drop(tx);
    let mut worker_rx = Some(rx);
    SyncCoordinator::process_worker_messages(state, repo, &mut worker_rx)
}

fn event(state: &mut AppState, repo: &ConfigRepository, event: SyncEvent) -> bool {
    apply(state, repo, WorkerMsg::SyncEvent(event))
}

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    (dir, repo, state)
}

fn progress(pct: Option<u32>, status: &str) -> WorkerMsg {
    WorkerMsg::Progress {
        pct,
        status: UiText::raw(status),
        file: UiText::raw("file"),
    }
}

fn cycle(peer: DeviceId, active: usize) -> SyncEvent {
    SyncEvent::SyncCycleCompleted {
        peer_device_id: peer,
        active_connections: active,
        files_up_to_date: 5,
        files_downloaded: 2,
        bytes_downloaded: 4096,
        blocks_uploaded: 1,
        bytes_uploaded: 128,
    }
}

#[test]
fn progress_never_moves_backwards_and_is_ignored_while_connected_or_initial_sync() {
    let (dir, repo, mut state) = setup("coord-progress-rules");
    assert_eq!(state.progress_pct, Some(0));

    assert!(apply(&mut state, &repo, progress(Some(40), "forty")));
    assert_eq!(state.progress_pct, Some(40));
    assert_eq!(state.status_line, UiText::raw("forty"));
    assert_eq!(state.current_file, UiText::raw("file"));

    apply(&mut state, &repo, progress(Some(30), "thirty"));
    assert_eq!(state.progress_pct, Some(40));
    assert_eq!(state.status_line, UiText::raw("forty"));

    // Equal percentages refresh the status.
    apply(&mut state, &repo, progress(Some(40), "still forty"));
    assert_eq!(state.status_line, UiText::raw("still forty"));

    // An indeterminate step never replaces a known percentage.
    apply(&mut state, &repo, progress(None, "connecting"));
    assert_eq!(state.progress_pct, Some(40));

    // A known percentage replaces the indeterminate state of 0 and of `None`.
    state.progress_pct = None;
    apply(&mut state, &repo, progress(Some(10), "ten"));
    assert_eq!(state.progress_pct, Some(10));
    state.progress_pct = Some(0);
    apply(&mut state, &repo, progress(None, "init"));
    assert_eq!(state.progress_pct, None);

    // While a session is connected, background retries do not touch the progress at all.
    state.progress_pct = Some(50);
    state.session_connected = true;
    apply(&mut state, &repo, progress(Some(90), "retry"));
    assert_eq!(state.progress_pct, Some(50));
    state.session_connected = false;

    // Same during the initial sync phase of a peer.
    state.initial_sync.begin(DeviceId([0x91; 32]));
    apply(&mut state, &repo, progress(Some(90), "retry"));
    assert_eq!(state.progress_pct, Some(50));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn discovered_device_is_listed_as_an_unpaired_peer() {
    let (dir, repo, mut state) = setup("coord-discovered");
    let id = DeviceId([0x92; 32]);
    let msg = WorkerMsg::DeviceDiscovered {
        device_id: id,
        name: String::new(),
        address: "192.168.1.30:22000".to_owned(),
    };
    assert!(apply(&mut state, &repo, msg));
    assert!(state.is_unpaired_lan_peer(&id));
    assert_eq!(state.devices[0].address, "192.168.1.30:22000");
    assert_eq!(state.devices[0].name, format!("LAN Peer ({})", id.short()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn finished_message_closes_the_worker_and_reports_its_summary() {
    let (dir, repo, mut state) = setup("coord-finished");
    state.sync_in_progress = true;
    state.uploading = true;
    state.session_connected = true;
    state.auto_reconnect_enabled = true;

    let (tx, rx) = mpsc::channel();
    tx.send(WorkerMsg::Finished {
        summary: UiText::raw("all done"),
        allow_auto_reconnect: false,
    })
    .unwrap();
    let mut worker_rx = Some(rx);
    assert!(SyncCoordinator::process_worker_messages(
        &mut state,
        &repo,
        &mut worker_rx
    ));
    assert!(worker_rx.is_none(), "the finished worker is forgotten");
    assert!(!state.sync_in_progress);
    assert!(!state.uploading);
    assert!(!state.session_connected);
    assert!(!state.auto_reconnect_enabled);
    assert_eq!(state.progress_pct, Some(100));
    assert_eq!(state.current_file, UiText::SyncFinished);
    assert_eq!(state.status_line, UiText::raw("all done"));

    // Nothing pending: nothing to redraw.
    let mut empty: Option<mpsc::Receiver<WorkerMsg>> = None;
    assert!(!SyncCoordinator::process_worker_messages(
        &mut state, &repo, &mut empty
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn peer_connected_names_the_peer_and_starts_the_index_exchange() {
    let (dir, repo, mut state) = setup("coord-peer-connected");
    let addr: SocketAddr = "10.0.0.7:22000".parse().unwrap();
    let id = DeviceId([0x93; 32]);

    event(
        &mut state,
        &repo,
        SyncEvent::PeerConnected {
            peer_addr: addr,
            peer_device_id: id,
            peer_name: String::new(),
            client_version: "v1.27".to_owned(),
        },
    );
    let unnamed = format!("Device {}", id.short());
    assert_eq!(state.devices[0].name, unnamed);
    assert_eq!(state.devices[0].address, "10.0.0.7:22000");
    assert_eq!(
        state.peer_label,
        UiText::raw(format!("{} @ 10.0.0.7:22000 (v1.27)", unnamed))
    );
    assert_eq!(state.status_line, UiText::ExchangingIndex);
    assert!(state.sync_in_progress);
    assert!(state.initial_sync.is_active());
    assert!(state
        .logs
        .iter()
        .any(|l| l == &format!("Connected via TLS 1.3 to {} (10.0.0.7:22000)", unnamed)));

    // A named peer keeps its name.
    let named = DeviceId([0x94; 32]);
    event(
        &mut state,
        &repo,
        SyncEvent::PeerConnected {
            peer_addr: addr,
            peer_device_id: named,
            peer_name: "NAS".to_owned(),
            client_version: "v2".to_owned(),
        },
    );
    assert_eq!(state.peer_label, UiText::raw("NAS @ 10.0.0.7:22000 (v2)"));
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Connected via TLS 1.3 to NAS (10.0.0.7:22000)"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cluster_config_turns_the_remote_folders_into_offers_and_a_label() {
    let (dir, repo, mut state) = setup("coord-cluster-config");
    let peer = DeviceId([0x95; 32]);
    event(
        &mut state,
        &repo,
        SyncEvent::ClusterConfigReceived {
            peer_device_id: peer,
            folders: vec![
                BepFolder {
                    id: "docs".to_owned(),
                    label: "Docs".to_owned(),
                    device_ids: Vec::new(),
                },
                BepFolder {
                    id: "pics".to_owned(),
                    label: String::new(),
                    device_ids: Vec::new(),
                },
            ],
        },
    );
    assert_eq!(state.folder_offers.len(), 2);
    assert_eq!(state.folder_offers[0].label, "Docs");
    assert_eq!(
        state.folder_offers[1].label, "pics",
        "folder ID is the fallback label"
    );
    assert_eq!(state.folder_label, UiText::raw("docs (Docs), pics ()"));
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Received shares from peer: docs (Docs), pics ()"));

    event(
        &mut state,
        &repo,
        SyncEvent::ClusterConfigReceived {
            peer_device_id: peer,
            folders: Vec::new(),
        },
    );
    assert_eq!(state.folder_label, UiText::NoRemoteFolders);
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Received shares from peer: No remote folders shared"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_events_update_the_counters_the_status_and_the_log() {
    let (dir, repo, mut state) = setup("coord-file-events");
    let logs_before = state.logs.len();

    event(
        &mut state,
        &repo,
        SyncEvent::FileUpToDate {
            folder: "docs".to_owned(),
            name: "a.txt".to_owned(),
            size: 3,
        },
    );
    assert_eq!(state.files_up_to_date, 1);
    assert_eq!(state.current_file, UiText::raw("a.txt"));
    assert_eq!(
        state.status_line,
        UiText::FileUpToDate {
            file: "a.txt".to_owned()
        }
    );
    assert_eq!(
        state.logs.len(),
        logs_before,
        "up-to-date files are not logged"
    );

    event(
        &mut state,
        &repo,
        SyncEvent::BlockDownloaded {
            folder: "docs".to_owned(),
            name: "b.bin".to_owned(),
            offset: 0,
            size: 250,
            total_size: 1000,
        },
    );
    assert!(state.sync_in_progress);
    assert_eq!(state.transfer_activity, 1);
    assert_eq!(state.bytes_transferred, 250);
    assert_eq!(state.progress_pct, Some(25));
    assert_eq!(
        state.status_line,
        UiText::Downloading {
            file: "b.bin".to_owned(),
            pct: 25
        }
    );

    // An unknown total counts as complete; the percentage never exceeds 100.
    event(
        &mut state,
        &repo,
        SyncEvent::BlockDownloaded {
            folder: "docs".to_owned(),
            name: "e.bin".to_owned(),
            offset: 0,
            size: 10,
            total_size: 0,
        },
    );
    assert_eq!(state.progress_pct, Some(100));
    event(
        &mut state,
        &repo,
        SyncEvent::BlockDownloaded {
            folder: "docs".to_owned(),
            name: "b.bin".to_owned(),
            offset: 900,
            size: 200,
            total_size: 1000,
        },
    );
    assert_eq!(state.progress_pct, Some(100));

    event(
        &mut state,
        &repo,
        SyncEvent::FileSynced {
            folder: "docs".to_owned(),
            name: "b.bin".to_owned(),
            blocks_downloaded: 4,
            bytes_downloaded: 1000,
            total_size: 1000,
        },
    );
    assert_eq!(state.files_downloaded, 1);
    assert_eq!(state.transfer_activity, 4);
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Synced: b.bin (4 block(s), 1000 bytes)"));

    event(
        &mut state,
        &repo,
        SyncEvent::LocalIndexPublished {
            folder: "docs".to_owned(),
            file_count: 7,
        },
    );
    assert_eq!(state.status_line, UiText::IndexPublished { files: 7 });
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Local index sent for docs (7 files)"));

    let logs_before = state.logs.len();
    event(
        &mut state,
        &repo,
        SyncEvent::BlockUploaded {
            folder: "docs".to_owned(),
            name: "c.bin".to_owned(),
            offset: 0,
            size: 128,
        },
    );
    assert!(state.uploading);
    assert_eq!(state.blocks_uploaded, 1);
    assert_eq!(state.bytes_transferred, 250 + 10 + 200 + 128);
    assert_eq!(state.current_file, UiText::raw("c.bin"));
    assert_eq!(
        state.status_line,
        UiText::Uploading {
            file: "c.bin".to_owned()
        }
    );
    assert_eq!(
        state.logs.len(),
        logs_before,
        "served blocks are not logged"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn completed_cycle_marks_the_session_connected_with_its_summary() {
    let (dir, repo, mut state) = setup("coord-cycle");
    let peer = DeviceId([0x96; 32]);
    state.auto_reconnect_enabled = false;

    event(&mut state, &repo, cycle(peer, 3));
    assert!(state.session_connected);
    assert!(state.connected_peers.is_active());
    assert!(!state.sync_in_progress);
    assert!(state.auto_reconnect_enabled);
    assert_eq!(state.progress_pct, Some(100));
    assert_eq!(state.current_file, UiText::Connections { count: 3 });
    assert_eq!(
        state.status_line,
        UiText::SyncSummary {
            connections: 3,
            up_to_date: 5,
            downloaded: 2,
            downloaded_kb: 4,
            uploaded: 1,
            uploaded_bytes: 128,
        }
    );
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Connected (1+2 connections) · 5 up-to-date | 2 DL (4 KB) | 1 UL (128 B)"));

    let other = DeviceId([0x97; 32]);
    event(&mut state, &repo, cycle(other, 1));
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Connected (1 connection) · 5 up-to-date | 2 DL (4 KB) | 1 UL (128 B)"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn session_end_goes_offline_only_when_the_last_connected_peer_leaves() {
    let (dir, repo, mut state) = setup("coord-session-end");
    let a = DeviceId([0x98; 32]);
    let b = DeviceId([0x99; 32]);
    event(&mut state, &repo, cycle(a, 1));
    event(&mut state, &repo, cycle(b, 1));
    state.uploading = true;
    state.sync_in_progress = true;

    event(
        &mut state,
        &repo,
        SyncEvent::SessionEnded {
            peer_device_id: a,
            reason: "closed".to_owned(),
        },
    );
    assert!(state.session_connected, "peer b is still connected");
    assert_eq!(state.progress_pct, Some(100));
    assert!(!state.sync_in_progress, "no initial sync is running");
    assert!(
        state.uploading,
        "offline state only starts with the last peer"
    );
    assert!(state
        .logs
        .iter()
        .any(|l| l == &format!("Session with {} ended (closed). Reconnecting...", a.short())));

    event(
        &mut state,
        &repo,
        SyncEvent::SessionEnded {
            peer_device_id: b,
            reason: "timeout".to_owned(),
        },
    );
    assert!(!state.session_connected);
    assert!(!state.uploading);
    assert_eq!(state.progress_pct, Some(0));
    assert_eq!(state.current_file, UiText::raw("-"));
    assert_eq!(state.status_line, UiText::WaitingForLan);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn folder_level_events_are_logged_with_their_folder_and_reverts_toast() {
    let (dir, repo, mut state) = setup("coord-folder-events");
    event(
        &mut state,
        &repo,
        SyncEvent::FolderUnavailable {
            folder: "docs".to_owned(),
            error: "no dir".to_owned(),
        },
    );
    assert!(state
        .logs
        .iter()
        .any(|l| l == "Folder docs skipped (local directory unavailable): no dir"));

    event(
        &mut state,
        &repo,
        SyncEvent::LocalChangeApplied {
            folder: "docs".to_owned(),
            description: "deleted a.txt".to_owned(),
        },
    );
    assert_eq!(state.transfer_activity, 1);
    assert!(state.logs.iter().any(|l| l == "[docs] deleted a.txt"));

    event(
        &mut state,
        &repo,
        SyncEvent::LocalChangesReverted {
            folder: "docs".to_owned(),
            reverted: 3,
        },
    );
    assert_eq!(state.transfer_activity, 2);
    assert!(state
        .logs
        .iter()
        .any(|l| l == "[docs] Local changes reverted (3 entries)"));
    assert_eq!(
        state.toast_msg.as_ref().map(|(m, _)| m.clone()),
        Some(UiText::LocalChangesReverted {
            id: "docs".to_owned(),
            count: 3
        })
    );

    event(
        &mut state,
        &repo,
        SyncEvent::FileError {
            folder: "docs".to_owned(),
            name: "x".to_owned(),
            error: "boom".to_owned(),
        },
    );
    assert!(state.logs.iter().any(|l| l == "Skipped file docs/x: boom"));
    let _ = std::fs::remove_dir_all(&dir);
}
