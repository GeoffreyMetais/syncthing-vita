//! Persistent session loop driven by the application's control handle: which cycles are
//! reported, when local changes are published, how a peer's `Close` ends the session, and which
//! revert requests a session takes.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use common::fake_peer::FakePeer;
use common::session_driver::{
    control_with, device_under_test, hello_under_test, index_messages, run_session,
};
use syncthing_core::{
    BepClose, BepFolder, BepMessageType, FolderType, FsSyncStorage, SyncControl, SyncEvent,
};

fn long_control() -> Arc<SyncControl> {
    control_with(Duration::from_secs(3600), Duration::from_secs(20))
}

/// Storage with the given folders, each with a local file. `linked` folders are shared with
/// `peer` and registered as local shares.
fn storage_with(name: &str, peer: &FakePeer, folders: &[&str], linked: &[&str]) -> FsSyncStorage {
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir(name));
    for folder in folders {
        if linked.contains(folder) {
            storage.set_folder_devices(folder, vec![peer.device_id]);
            storage.add_local_folder(BepFolder {
                id: (*folder).to_owned(),
                label: (*folder).to_owned(),
                device_ids: vec![peer.device_id],
                ..Default::default()
            });
        }
        let dir = storage.folder_root(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("local.txt"), folder.as_bytes()).unwrap();
    }
    storage
}

fn cycles(events: &[SyncEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, SyncEvent::SyncCycleCompleted { .. }))
        .count()
}

#[test]
fn test_idle_session_reports_only_its_initial_cycle() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x81, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        // An index of a folder that is not shared with us is ignored: no cycle for it.
        let secret = conn.file("secret", "s.txt", b"not for us", 1);
        conn.send_index("secret", vec![secret], true);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let storage = storage_with(
        "control_idle_cycles",
        &peer,
        &["shared", "secret"],
        &["shared"],
    );
    let mut events = Vec::new();
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(long_control()),
        |e| events.push(e),
    )
    .expect("session failed");
    peer.join();

    assert_eq!(cycles(&events), 1);
    assert!(!storage.folder_root("secret").join("s.txt").exists());
}

#[test]
fn test_manual_rescan_reports_a_cycle_even_without_changes() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x82, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let storage = storage_with("control_manual_rescan", &peer, &["shared"], &["shared"]);
    let control = long_control();
    let requester = Arc::clone(&control);
    let mut events = Vec::new();
    let mut asked = false;
    run_session(&peer, &storage, &hello_under_test(1), Some(control), |e| {
        // Once the initial cycle is reported, the application asks for a rescan.
        if !asked && matches!(e, SyncEvent::SyncCycleCompleted { .. }) {
            asked = true;
            requester.request_rescan();
        }
        events.push(e);
    })
    .expect("session failed");
    peer.join();

    assert_eq!(
        cycles(&events),
        2,
        "the manual rescan is a cycle of its own"
    );
}

#[test]
fn test_local_change_is_published_by_the_next_periodic_rescan() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x83, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        // Returns as soon as the change is published (or after the timeout).
        conn.wait_for(BepMessageType::IndexUpdate, Duration::from_secs(6));
    });

    let storage = storage_with("control_periodic_rescan", &peer, &["shared"], &["shared"]);
    let late = storage.resolve_safe_path("shared", "late.txt");
    // Written well after the initial cycle: only the periodic rescan can publish it.
    let writer = {
        let late = late.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(2000));
            std::fs::write(late, b"created later").unwrap();
        })
    };
    let control = control_with(Duration::from_millis(300), Duration::from_secs(20));
    run_session(&peer, &storage, &hello_under_test(1), Some(control), |_| {})
        .expect("session failed");
    let received = peer.join();
    writer.join().unwrap();

    let published = index_messages(&received)
        .into_iter()
        .any(|index| index.files.iter().any(|f| f.name == "late.txt"));
    assert!(
        published,
        "the periodic rescan did not publish the local change"
    );
}

#[test]
fn test_close_from_the_peer_ends_the_persistent_session() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x84, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        // Once the initial cycle is over, the peer closes the session and stays connected.
        thread::sleep(Duration::from_millis(1500));
        conn.send(
            BepMessageType::Close,
            &BepClose {
                reason: "bye".to_owned(),
            }
            .encode(),
        );
        thread::sleep(Duration::from_millis(1000));
    });

    let storage = storage_with("control_peer_close", &peer, &["shared"], &["shared"]);
    let mut events = Vec::new();
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(long_control()),
        |e| events.push(e),
    )
    .expect("session failed");
    peer.join();

    let reason = events.iter().find_map(|e| match e {
        SyncEvent::SessionEnded { reason, .. } => Some(reason.clone()),
        _ => None,
    });
    assert_eq!(reason.as_deref(), Some("closed by peer: bye"));
}

#[test]
fn test_only_linked_running_receive_only_folders_take_a_revert() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x85, move |conn| {
        conn.send_cluster_config(&["rw", "ro_paused"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("rw", Vec::new(), false);
        conn.send_index("ro_paused", Vec::new(), false);
        // Stay connected past the drain window: the persistent loop handles the requests.
        thread::sleep(Duration::from_millis(3500));
    });

    // `rw`: read & write, linked. `ro_paused`: receive-only, linked, paused. `ro_unlinked`:
    // receive-only, not shared with the peer at all.
    let mut storage = storage_with(
        "control_revert_filter",
        &peer,
        &["rw", "ro_paused", "ro_unlinked"],
        &["rw", "ro_paused"],
    );
    storage.set_folder_type("ro_paused", FolderType::ReceiveOnly);
    storage.set_folder_paused("ro_paused", true);
    storage.set_folder_type("ro_unlinked", FolderType::ReceiveOnly);

    let control = long_control();
    for folder in ["rw", "ro_paused", "ro_unlinked"] {
        control.request_revert(folder);
    }
    let mut events = Vec::new();
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(Arc::clone(&control)),
        |e| events.push(e),
    )
    .expect("session failed");
    peer.join();

    // No session took the requests, and nothing was reverted.
    let pending = control.take_revert_requests(|_| true);
    assert_eq!(pending, vec!["rw", "ro_paused", "ro_unlinked"]);
    let touched = events.iter().any(|e| {
        matches!(
            e,
            SyncEvent::LocalChangesReverted { .. } | SyncEvent::FolderUnavailable { .. }
        )
    });
    assert!(!touched, "a folder that cannot be reverted was reverted");
    for folder in ["rw", "ro_paused", "ro_unlinked"] {
        assert!(storage.folder_root(folder).join("local.txt").exists());
    }
}

#[test]
fn test_no_periodic_rescan_before_the_interval_elapses() {
    let vita = device_under_test().device_id;
    let published = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&published);
    let peer = FakePeer::spawn(0x86, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        // A local change made during the session is not published before the rescan interval.
        let update = conn.wait_for(BepMessageType::IndexUpdate, Duration::from_millis(2500));
        flag.store(update.is_some(), Ordering::SeqCst);
    });

    let storage = storage_with("control_no_early_rescan", &peer, &["shared"], &["shared"]);
    let late = storage.resolve_safe_path("shared", "late.txt");
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1200));
        std::fs::write(late, b"created during the session").unwrap();
    });
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(long_control()),
        |_| {},
    )
    .expect("session failed");
    peer.join();
    writer.join().unwrap();

    assert!(
        !published.load(Ordering::SeqCst),
        "a rescan ran before its interval"
    );
}
