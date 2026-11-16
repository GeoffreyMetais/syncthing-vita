//! Secondary data connections (`1+2`): how many are opened for a two-connection peer, when our
//! index waits for their promotion, how the persistent loop serves and pings them, and how
//! dropped ones are re-opened.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use common::fake_peer::{
    control_with, device_under_test, hello_under_test, payloads, run_session, FakePeer,
};
use syncthing_core::{BepFolder, BepMessageType, BepResponse, FsSyncStorage, SyncEvent};

/// Storage sharing `shared` with `peer`, with a local file so that it has an index to publish.
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

/// Script of the primary connection: exchanges `ClusterConfig`s, sends an empty index, waits
/// for ours, then stays connected for `stay`.
fn primary_script(
    stay: Duration,
) -> impl FnOnce(&mut common::fake_peer::PeerConn) + Send + 'static {
    let vita = device_under_test().device_id;
    move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(10))
            .expect("Index of the shared folder");
        thread::sleep(stay);
    }
}

#[test]
fn test_one_secondary_connection_is_opened_for_a_two_connection_peer() {
    let peer = FakePeer::spawn_with_secondaries(
        0x91,
        2,
        primary_script(Duration::from_millis(1500)),
        |conn, _| conn.serve_until_idle(Duration::from_millis(1000)),
    );
    let storage = storage_for("secondary_count", &peer);
    let control = control_with(Duration::from_secs(3600), Duration::from_secs(20));
    run_session(&peer, &storage, &hello_under_test(2), Some(control), |_| {})
        .expect("session failed");
    let accepted = peer.accepted.load(Ordering::SeqCst);
    peer.join_with_secondaries();

    // Both sides announce two connections: the device opens exactly one secondary connection.
    assert_eq!(accepted, 1);
}

#[test]
fn test_index_is_published_once_the_secondary_connection_is_promoted() {
    let promoted = Arc::new(AtomicBool::new(false));
    let promoted_by_peer = Arc::clone(&promoted);
    let saw_promotion = Arc::new(AtomicBool::new(false));
    let saw = Arc::clone(&saw_promotion);
    let peer = FakePeer::spawn_with_secondaries(
        0x92,
        2,
        move |conn| {
            let vita = device_under_test().device_id;
            conn.send_cluster_config(&["shared"], vita);
            conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
                .expect("ClusterConfig reply");
            conn.send_index("shared", Vec::new(), false);
            conn.wait_for(BepMessageType::Index, Duration::from_secs(10))
                .expect("Index of the shared folder");
            saw.store(promoted.load(Ordering::SeqCst), Ordering::SeqCst);
            thread::sleep(Duration::from_millis(500));
        },
        move |conn, _| {
            // The peer promotes the secondary connection after a while.
            thread::sleep(Duration::from_millis(600));
            promoted_by_peer.store(true, Ordering::SeqCst);
            conn.promote();
            conn.serve_until_idle(Duration::from_millis(800));
        },
    );
    let storage = storage_for("secondary_promotion", &peer);
    let control = control_with(Duration::from_secs(3600), Duration::from_secs(20));
    run_session(&peer, &storage, &hello_under_test(2), Some(control), |_| {})
        .expect("session failed");
    peer.join_with_secondaries();

    assert!(
        saw_promotion.load(Ordering::SeqCst),
        "our index was published before the secondary connection was promoted"
    );
}

#[test]
fn test_secondary_connection_serves_requests_and_receives_pings() {
    let answered = Arc::new(AtomicBool::new(false));
    let answered_flag = Arc::clone(&answered);
    let peer = FakePeer::spawn_with_secondaries(
        0x93,
        2,
        primary_script(Duration::from_millis(3500)),
        move |conn, _| {
            conn.promote();
            // Asked after the initial cycle, so the persistent loop must serve it.
            thread::sleep(Duration::from_millis(2200));
            let response: Option<BepResponse> =
                conn.request(55, "shared", "local.txt", Duration::from_secs(3));
            answered_flag.store(
                response.is_some_and(|r| r.id == 55 && r.code == 0),
                Ordering::SeqCst,
            );
            // Keepalives keep arriving: wait by time, not by silence.
            thread::sleep(Duration::from_millis(300));
        },
    );
    let storage = storage_for("secondary_requests", &peer);
    // Keepalives every 50 ms: the persistent loop must ping the secondary connection too.
    let control = control_with(Duration::from_secs(3600), Duration::from_millis(50));
    run_session(&peer, &storage, &hello_under_test(2), Some(control), |_| {})
        .expect("session failed");
    let (_, secondaries) = peer.join_with_secondaries();

    assert!(
        answered.load(Ordering::SeqCst),
        "the secondary request was not answered"
    );
    let pings = secondaries
        .iter()
        .map(|received| payloads(received, BepMessageType::Ping).len())
        .sum::<usize>();
    assert!(pings > 0, "no keepalive reached the secondary connection");
}

#[test]
fn test_cycles_report_the_connections_in_use() {
    // An unpromoted secondary makes the initial index wait for the promotion budget, then the
    // initial cycle is reported after one more second of drain: keep both connections open
    // through both cycles.
    let peer = FakePeer::spawn_with_secondaries(
        0x94,
        2,
        primary_script(Duration::from_millis(3500)),
        |_conn, _| thread::sleep(Duration::from_millis(4000)),
    );
    let storage = storage_for("secondary_cycles", &peer);
    let control = control_with(Duration::from_secs(3600), Duration::from_secs(20));
    let requester = Arc::clone(&control);
    let mut cycles = Vec::new();
    let mut asked = false;
    run_session(&peer, &storage, &hello_under_test(2), Some(control), |e| {
        if let SyncEvent::SyncCycleCompleted {
            active_connections, ..
        } = e
        {
            cycles.push(active_connections);
            if !asked {
                asked = true;
                requester.request_rescan();
            }
        }
    })
    .expect("session failed");
    peer.join_with_secondaries();

    // The initial cycle and the manual rescan both run over the primary and the secondary.
    assert_eq!(cycles, vec![2, 2]);
}

#[test]
fn test_dropped_secondary_connection_is_not_reopened_at_once() {
    // The secondary connection is closed right away: the session loses it.
    let peer = FakePeer::spawn_with_secondaries(
        0x95,
        2,
        primary_script(Duration::from_millis(2500)),
        |_, _| {},
    );
    let storage = storage_for("secondary_reopen", &peer);
    let control = control_with(Duration::from_secs(3600), Duration::from_secs(20));
    run_session(&peer, &storage, &hello_under_test(2), Some(control), |_| {})
        .expect("session failed");
    let accepted = peer.accepted.load(Ordering::SeqCst);
    peer.join_with_secondaries();

    // Re-opening is attempted only after its long retry interval, not during the session.
    assert_eq!(
        accepted, 1,
        "the secondary connection was re-opened too early"
    );
}
