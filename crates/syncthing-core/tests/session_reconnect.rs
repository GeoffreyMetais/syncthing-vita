//! Retry of secondary connections in the persistent loop. The retry interval is long (15 s), so
//! these sessions last a little over that interval: they run in parallel.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use common::fake_peer::{FakePeer, PeerConn};
use common::session_driver::{control_with, device_under_test, hello_under_test, run_session};
use syncthing_core::{BepFolder, BepMessageType, FsSyncStorage};

/// Longer than the secondary retry interval of the persistent loop.
const SESSION: Duration = Duration::from_millis(17_000);

/// A healthy secondary connection outlives the primary one: its script starts before the
/// primary's `ClusterConfig` / `Index` exchange, which can take seconds on a loaded machine.
const SECONDARY_MARGIN: Duration = Duration::from_secs(10);

fn storage_for(name: &str, peer: &FakePeer) -> FsSyncStorage {
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir(name));
    storage.set_folder_devices("shared", vec![peer.device_id]);
    storage.add_local_folder(BepFolder {
        id: "shared".to_owned(),
        label: "shared".to_owned(),
        device_ids: vec![peer.device_id],
    });
    let dir = storage.folder_root("shared");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("local.txt"), b"vita").unwrap();
    storage
}

/// Primary connection of a two-connection peer: stays connected for `SESSION`.
fn primary(conn: &mut PeerConn) {
    let vita = device_under_test().device_id;
    conn.send_cluster_config(&["shared"], vita);
    conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
        .expect("ClusterConfig reply");
    conn.send_index("shared", Vec::new(), false);
    conn.wait_for(BepMessageType::Index, Duration::from_secs(10))
        .expect("Index of the shared folder");
    thread::sleep(SESSION);
}

fn run_long_session(peer: &FakePeer, storage: &FsSyncStorage) {
    let control = control_with(Duration::from_secs(3600), Duration::from_secs(20));
    run_session(peer, storage, &hello_under_test(2), Some(control), |_| {})
        .expect("session failed");
}

#[test]
fn test_healthy_secondary_connection_is_not_reopened() {
    let peer = FakePeer::spawn_with_secondaries(0xA1, 2, primary, |conn, _| {
        conn.promote();
        thread::sleep(SESSION + SECONDARY_MARGIN);
    });
    let storage = storage_for("reconnect_healthy", &peer);
    run_long_session(&peer, &storage);
    let accepted = peer.accepted.load(Ordering::SeqCst);
    peer.join_with_secondaries();
    assert_eq!(
        accepted, 1,
        "a healthy secondary connection was opened again"
    );
}

#[test]
fn test_dropped_secondary_connection_is_reopened_after_the_retry_interval() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let peer = FakePeer::spawn_with_secondaries(0xA2, 2, primary, move |_, _| {
        // Every secondary connection is closed right away.
        counted.fetch_add(1, Ordering::SeqCst);
    });
    let storage = storage_for("reconnect_dropped", &peer);
    run_long_session(&peer, &storage);
    let accepted = peer.accepted.load(Ordering::SeqCst);
    peer.join_with_secondaries();
    assert_eq!(
        accepted, 2,
        "the dropped secondary connection was not re-opened"
    );
}
