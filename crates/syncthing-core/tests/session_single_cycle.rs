//! Single-cycle sessions (`SyncControl::single_cycle`), as run by a background worker: one
//! sync cycle, a drain that lasts while the peer is active, a clean `Close`, and stop requests
//! honoured at every phase.

mod common;

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use common::fake_peer::FakePeer;
use common::session_driver::{device_under_test, hello_under_test, run_session, tuning};
use syncthing_core::{
    BepFolder, BepMessageType, CycleDrain, FsSyncStorage, SyncControl, SyncEvent,
    SyncSessionStatus, SyncTuning,
};

const FOLDER: &str = "shared";

fn single_cycle(idle: Duration, max: Duration) -> Arc<SyncControl> {
    SyncControl::single_cycle(
        tuning(Duration::from_secs(3600), Duration::from_secs(20)),
        CycleDrain { idle, max },
    )
}

/// Storage sharing `FOLDER` with `peer`, holding one local file `local.txt`.
fn storage_for(name: &str, peer: &FakePeer) -> FsSyncStorage {
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir(name));
    storage.set_folder_devices(FOLDER, vec![peer.device_id]);
    storage.add_local_folder(BepFolder {
        id: FOLDER.to_owned(),
        label: FOLDER.to_owned(),
        device_ids: vec![peer.device_id],
    });
    let dir = storage.folder_root(FOLDER);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("local.txt"), b"local content").unwrap();
    storage
}

fn ended_reason(events: &[SyncEvent]) -> Option<String> {
    events.iter().find_map(|e| match e {
        SyncEvent::SessionEnded { reason, .. } => Some(reason.clone()),
        _ => None,
    })
}

fn cycles(events: &[SyncEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, SyncEvent::SyncCycleCompleted { .. }))
        .count()
}

#[test]
fn test_control_handles_know_whether_they_run_a_single_cycle() {
    let drain = CycleDrain {
        idle: Duration::from_secs(2),
        max: Duration::from_secs(9),
    };
    let tuned = SyncTuning {
        rescan_interval: Duration::from_secs(7),
        keepalive_interval: Duration::from_secs(5),
    };
    let control = SyncControl::single_cycle(tuned, drain);
    assert_eq!(control.single_cycle_drain(), Some(drain));
    assert_eq!(control.tuning(), tuned);
    assert!(!control.is_stop_requested());
    assert_eq!(SyncControl::with_tuning(tuned).single_cycle_drain(), None);
    assert_eq!(SyncControl::new().single_cycle_drain(), None);
}

#[test]
fn test_cycle_drain_ends_on_silence_or_at_its_bound() {
    let drain = CycleDrain {
        idle: Duration::from_secs(2),
        max: Duration::from_secs(10),
    };
    let s = Duration::from_secs;
    assert!(!drain.is_over(s(1), s(1)));
    assert!(!drain.is_over(s(9), Duration::from_millis(1999)));
    assert!(drain.is_over(s(3), s(2)), "silent for `idle`");
    assert!(drain.is_over(s(10), s(0)), "bounded by `max`");
    assert!(drain.is_over(s(11), s(0)));
    let fixed = CycleDrain::fixed(s(4));
    assert_eq!(fixed.idle, s(4));
    assert_eq!(fixed.max, s(4));
}

#[test]
fn test_single_cycle_pulls_publishes_and_closes() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x91, move |conn| {
        conn.send_cluster_config(&[FOLDER], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        let remote = conn.file(FOLDER, "remote.txt", b"from the peer", 1);
        conn.send_index(FOLDER, vec![remote], false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        conn.wait_for(BepMessageType::Close, Duration::from_secs(5))
            .expect("Close at the end of the cycle");
    });

    let storage = storage_for("single_cycle_pull", &peer);
    let control = single_cycle(Duration::from_millis(300), Duration::from_secs(10));
    let mut events = Vec::new();
    let started = Instant::now();
    let result = run_session(&peer, &storage, &hello_under_test(1), Some(control), |e| {
        events.push(e)
    })
    .expect("session failed");
    let elapsed = started.elapsed();
    peer.join();

    assert!(matches!(
        result.session_status,
        SyncSessionStatus::Active {
            files_downloaded: 1,
            ..
        }
    ));
    assert_eq!(
        std::fs::read(storage.folder_root(FOLDER).join("remote.txt")).unwrap(),
        b"from the peer"
    );
    assert_eq!(cycles(&events), 1);
    assert_eq!(
        ended_reason(&events).as_deref(),
        Some("sync cycle completed")
    );
    assert!(
        elapsed < Duration::from_secs(8),
        "the idle drain took {elapsed:?}"
    );
}

#[test]
fn test_drain_lasts_while_the_peer_keeps_requesting() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x92, move |conn| {
        conn.send_cluster_config(&[FOLDER], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index(FOLDER, Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        // Requests spaced by less than the idle window, over three idle windows in total.
        let mut answered = 0;
        for id in 1..=6 {
            thread::sleep(Duration::from_millis(300));
            if conn
                .request(id, FOLDER, "local.txt", Duration::from_secs(2))
                .is_some()
            {
                answered += 1;
            }
        }
        assert_eq!(answered, 6, "every request of the drain is answered");
        conn.wait_for(BepMessageType::Close, Duration::from_secs(5))
            .expect("Close once the peer is idle");
    });

    let storage = storage_for("single_cycle_drain_active", &peer);
    let control = single_cycle(Duration::from_millis(700), Duration::from_secs(20));
    let mut events = Vec::new();
    run_session(&peer, &storage, &hello_under_test(1), Some(control), |e| {
        events.push(e)
    })
    .expect("session failed");
    peer.join();

    assert_eq!(cycles(&events), 1);
    assert_eq!(
        ended_reason(&events).as_deref(),
        Some("sync cycle completed")
    );
}

#[test]
fn test_drain_never_exceeds_its_bound() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x93, move |conn| {
        conn.send_cluster_config(&[FOLDER], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index(FOLDER, Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        // A peer that never goes quiet: only the bound ends the drain.
        let started = Instant::now();
        let mut id = 1;
        while started.elapsed() < Duration::from_secs(8) {
            if conn
                .request(id, FOLDER, "local.txt", Duration::from_millis(500))
                .is_none()
            {
                break;
            }
            id += 1;
            thread::sleep(Duration::from_millis(100));
        }
    });

    let storage = storage_for("single_cycle_drain_bound", &peer);
    let control = single_cycle(Duration::from_millis(600), Duration::from_millis(1500));
    let mut published_at = None;
    let mut ended_at = None;
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(control),
        |e| match e {
            SyncEvent::LocalIndexPublished { .. } => published_at = Some(Instant::now()),
            SyncEvent::SessionEnded { .. } => ended_at = Some(Instant::now()),
            _ => {}
        },
    )
    .expect("session failed");
    peer.join();

    let drained = ended_at.unwrap() - published_at.unwrap();
    assert!(
        drained >= Duration::from_millis(1400) && drained < Duration::from_secs(4),
        "the drain lasted {drained:?}"
    );
}

#[test]
fn test_stop_during_the_drain_ends_the_cycle_without_reporting_it() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x94, move |conn| {
        conn.send_cluster_config(&[FOLDER], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index(FOLDER, Vec::new(), false);
        conn.wait_for(BepMessageType::Close, Duration::from_secs(10))
            .expect("Close after the stop request");
    });

    let storage = storage_for("single_cycle_stop_drain", &peer);
    let control = single_cycle(Duration::from_secs(30), Duration::from_secs(60));
    let stopper = Arc::clone(&control);
    let mut events = Vec::new();
    let started = Instant::now();
    run_session(&peer, &storage, &hello_under_test(1), Some(control), |e| {
        if matches!(e, SyncEvent::LocalIndexPublished { .. }) {
            stopper.request_stop();
        }
        events.push(e);
    })
    .expect("session failed");
    peer.join();

    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(cycles(&events), 0);
    assert_eq!(ended_reason(&events).as_deref(), Some("stopped locally"));
}

#[test]
fn test_stop_before_the_download_skips_it() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x95, move |conn| {
        conn.send_cluster_config(&[FOLDER], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        let remote = conn.file(FOLDER, "remote.txt", b"never pulled", 1);
        conn.send_index(FOLDER, vec![remote], false);
        conn.wait_for(BepMessageType::Close, Duration::from_secs(10))
            .expect("Close after the stop request");
    });

    let storage = storage_for("single_cycle_stop_early", &peer);
    let control = single_cycle(Duration::from_secs(30), Duration::from_secs(60));
    let stopper = Arc::clone(&control);
    let mut events = Vec::new();
    let result = run_session(&peer, &storage, &hello_under_test(1), Some(control), |e| {
        if matches!(e, SyncEvent::PeerConnected { .. }) {
            stopper.request_stop();
        }
        events.push(e);
    });
    let received = peer.join();

    // The stop is seen inside the download (an error) or right after it.
    let downloaded = events
        .iter()
        .any(|e| matches!(e, SyncEvent::FileSynced { .. }));
    assert!(!downloaded, "a file was pulled after the stop request");
    assert!(!storage.folder_root(FOLDER).join("remote.txt").exists());
    assert!(
        !received
            .iter()
            .any(|(t, _)| *t == BepMessageType::Index || *t == BepMessageType::IndexUpdate),
        "nothing is published after a stop request"
    );
    if let Ok(result) = result {
        assert!(matches!(
            result.session_status,
            SyncSessionStatus::Active { .. }
        ));
        assert_eq!(ended_reason(&events).as_deref(), Some("stopped locally"));
    }
}

#[test]
fn test_persistent_handle_still_runs_the_persistent_loop() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x96, move |conn| {
        conn.send_cluster_config(&[FOLDER], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index(FOLDER, Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        // Well past the persistent drain: a persistent session is still connected.
        thread::sleep(Duration::from_millis(2500));
        assert!(
            conn.request(1, FOLDER, "local.txt", Duration::from_secs(2))
                .is_some(),
            "the persistent session no longer answers"
        );
    });

    let storage = storage_for("single_cycle_persistent", &peer);
    let control =
        SyncControl::with_tuning(tuning(Duration::from_secs(3600), Duration::from_secs(20)));
    let mut events = Vec::new();
    run_session(&peer, &storage, &hello_under_test(1), Some(control), |e| {
        events.push(e)
    })
    .expect("session failed");
    peer.join();

    assert!(ended_reason(&events)
        .as_deref()
        .is_some_and(|r| r != "sync cycle completed"));
}

#[test]
fn test_download_failure_without_a_stop_is_still_an_error() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x97, move |conn| {
        conn.send_cluster_config(&[FOLDER], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        let mut remote = conn.file(FOLDER, "corrupt.txt", b"served content", 1);
        // The announced hash does not match what is served.
        remote.blocks[0].hash = vec![0u8; 32];
        conn.send_index(FOLDER, vec![remote], false);
        conn.serve_until_idle(Duration::from_secs(2));
    });

    let storage = storage_for("single_cycle_corrupt", &peer);
    let control = single_cycle(Duration::from_millis(300), Duration::from_secs(10));
    let mut events = Vec::new();
    let result = run_session(&peer, &storage, &hello_under_test(1), Some(control), |e| {
        events.push(e)
    });
    peer.join();

    let err = result.expect_err("an invalid block is a protocol error, not a stop");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(ended_reason(&events), None);
    assert!(!storage.folder_root(FOLDER).join("corrupt.txt").exists());
}
