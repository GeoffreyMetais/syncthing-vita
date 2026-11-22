//! Block transfers of a session: pull statistics, requests and pings answered while a download
//! waits for its block, block hash verification, and keepalives sent during a local scan.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use common::fake_peer::FakePeer;
use common::session_driver::{
    control_with, device_under_test, hello_under_test, payloads, run_session,
};
use syncthing_core::{
    BepMessageType, BepResponse, FsSyncStorage, SyncEvent, SyncSessionStatus, BEP_BLOCK_SIZE,
};

/// Vita-side device ID of the test identity.
fn vita_id() -> syncthing_core::DeviceId {
    device_under_test().device_id
}

/// A persistent control that never rescans or pings on its own: the session ends when the
/// peer disconnects.
fn quiet_control() -> Arc<syncthing_core::SyncControl> {
    control_with(Duration::from_secs(3600), Duration::from_secs(20))
}

#[test]
fn test_up_to_date_files_and_downloaded_bytes_are_counted() {
    let vita = vita_id();
    let peer = FakePeer::spawn(0x61, move |conn| {
        let same = conn.file("saves", "same.bin", b"already here", 1);
        let new = conn.file("saves", "new.bin", b"downloaded now", 1);
        conn.send_cluster_config(&["saves"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("saves", vec![same, new], false);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("transfers_counters"));
    storage.set_folder_devices("saves", vec![peer.device_id]);
    let dir = storage.folder_root("saves");
    std::fs::create_dir_all(&dir).unwrap();
    // Identical to the peer's copy: nothing to download.
    std::fs::write(dir.join("same.bin"), b"already here").unwrap();

    let mut events = Vec::new();
    let result = run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(quiet_control()),
        |e| events.push(e),
    )
    .expect("session failed");
    peer.join();

    let SyncSessionStatus::Active {
        files_up_to_date,
        files_downloaded,
        bytes_downloaded,
        ..
    } = result.session_status
    else {
        panic!("session should be active");
    };
    assert_eq!(files_up_to_date, 1);
    assert_eq!(files_downloaded, 1);
    assert_eq!(bytes_downloaded, b"downloaded now".len() as u64);

    let synced = events.iter().find_map(|e| match e {
        SyncEvent::FileSynced {
            name,
            blocks_downloaded,
            bytes_downloaded,
            ..
        } if name == "new.bin" => Some((*blocks_downloaded, *bytes_downloaded)),
        _ => None,
    });
    assert_eq!(synced, Some((1, b"downloaded now".len() as u64)));
    assert_eq!(
        std::fs::read(storage.resolve_safe_path("saves", "new.bin")).unwrap(),
        b"downloaded now"
    );
}

#[test]
fn test_requests_and_pings_are_answered_while_a_block_is_awaited() {
    let vita = vita_id();
    let peer = FakePeer::spawn(0x62, move |conn| {
        let file = conn.file("saves", "a.bin", b"first file", 1);
        conn.send_cluster_config(&["saves"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("saves", vec![file], false);
        // The device asks for our block: it is waiting for its own block response.
        let pending = conn
            .wait_for(BepMessageType::Request, Duration::from_secs(5))
            .expect("block request");
        // Meanwhile the device asks us for one of its blocks, and pings us.
        let answer = conn
            .request(77, "saves", "other.bin", Duration::from_secs(3))
            .expect("answer to the device's request");
        assert_eq!(answer.id, 77);
        conn.send(BepMessageType::Ping, &[]);
        conn.wait_for(BepMessageType::Ping, Duration::from_secs(3))
            .expect("ping answered");
        conn.answer(&pending);
        conn.serve_until_idle(Duration::from_millis(800));
    });

    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("transfers_interleaved"));
    storage.set_folder_devices("saves", vec![peer.device_id]);
    let result = run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(quiet_control()),
        |_| {},
    )
    .expect("session failed");
    let received = peer.join();

    assert_eq!(
        std::fs::read(storage.resolve_safe_path("saves", "a.bin")).unwrap(),
        b"first file"
    );
    assert!(matches!(
        result.session_status,
        SyncSessionStatus::Active {
            files_downloaded: 1,
            ..
        }
    ));
    // The device's own request was answered with a response to its id.
    let responses = payloads(&received, BepMessageType::Response);
    assert!(responses
        .iter()
        .any(|p| BepResponse::decode(p).unwrap().id == 77));
}

#[test]
fn test_block_with_a_wrong_sha256_is_refused() {
    let vita = vita_id();
    let peer = FakePeer::spawn(0x63, move |conn| {
        let mut bad = conn.file("saves", "bad.bin", b"tampered", 1);
        // A 32-byte hash that does not match the served content.
        bad.blocks[0].hash = vec![0u8; 32];
        conn.send_cluster_config(&["saves"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("saves", vec![bad], false);
        conn.serve_until_idle(Duration::from_millis(800));
    });

    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("transfers_bad_hash"));
    storage.set_folder_devices("saves", vec![peer.device_id]);
    let path = storage.resolve_safe_path("saves", "bad.bin");

    let result = run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(quiet_control()),
        |_| {},
    );
    peer.join();
    assert!(
        result.is_err(),
        "a block that fails its hash ends the session"
    );
    assert!(!path.exists(), "the unverified content must not be written");
}

#[test]
fn test_block_with_a_hash_that_is_not_a_sha256_is_never_requested_nor_written() {
    let vita = vita_id();
    let peer = FakePeer::spawn(0x64, move |conn| {
        let mut bad = conn.file("saves", "short.bin", b"unverifiable", 1);
        // A 2-byte hash: nothing could ever be checked against it.
        bad.blocks[0].hash = vec![0xAB, 0xCD];
        conn.send_cluster_config(&["saves"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("saves", vec![bad], false);
        conn.serve_until_idle(Duration::from_millis(800));
    });

    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("transfers_short_hash"));
    storage.set_folder_devices("saves", vec![peer.device_id]);
    let path = storage.resolve_safe_path("saves", "short.bin");

    let _ = run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(quiet_control()),
        |_| {},
    );
    let received = peer.join();
    assert!(
        payloads(&received, BepMessageType::Request).is_empty(),
        "no block of an unverifiable file is requested"
    );
    assert!(!path.exists(), "the unverifiable file must not be written");
}

#[test]
fn test_keepalives_are_sent_during_a_long_local_scan() {
    let vita = vita_id();
    let pings_before_index = Arc::new(AtomicUsize::new(usize::MAX));
    let observed = Arc::clone(&pings_before_index);
    let peer = FakePeer::spawn(0x64, move |conn| {
        conn.send_cluster_config(&["saves"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("saves", Vec::new(), false);
        // Our index comes once the local folder is scanned (hashed).
        conn.wait_for(BepMessageType::Index, Duration::from_secs(20))
            .expect("local Index");
        let pings = payloads(&conn.received, BepMessageType::Ping).len();
        observed.store(pings, Ordering::SeqCst);
        conn.serve_until_idle(Duration::from_millis(200));
    });

    // Four MiB of local content: hashing it takes long enough to be interrupted by keepalives.
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("transfers_scan_keepalive"));
    storage.set_folder_devices("saves", vec![peer.device_id]);
    let dir = storage.folder_root("saves");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("big.bin"), vec![0xAB; 4 << 20]).unwrap();

    let control = control_with(Duration::from_secs(3600), Duration::from_millis(5));
    run_session(&peer, &storage, &hello_under_test(1), Some(control), |_| {})
        .expect("session failed");
    peer.join();
    assert!(
        pings_before_index.load(Ordering::SeqCst) > 0,
        "no keepalive was sent while the folder was being scanned"
    );
}

#[test]
fn test_blocks_already_held_locally_are_kept_and_only_the_others_are_fetched() {
    let vita = vita_id();
    let first = vec![0xAA_u8; BEP_BLOCK_SIZE];
    let second = vec![0xBB_u8; BEP_BLOCK_SIZE];
    let served: Vec<u8> = [first.clone(), second.clone()].concat();
    let peer = FakePeer::spawn(0x65, move |conn| {
        let mut file = conn.file_in_blocks("saves", "world.bin", &served, 2, BEP_BLOCK_SIZE);
        file.modified_s = 4_000_000_000;
        // The peer's version has seen our own counter: the local copy is an older version of
        // the file, not a conflicting one, so its blocks are kept.
        let own = u64::from_be_bytes(vita.0[..8].try_into().unwrap());
        file.versions.push(syncthing_core::BepVersionCounter {
            short_device_id: own,
            // Larger than the timestamp our scan records for the local copy.
            value: 1 << 40,
        });
        conn.send_cluster_config(&["saves"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("saves", vec![file], false);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("transfers_partial_blocks"));
    storage.set_folder_devices("saves", vec![peer.device_id]);
    let dir = storage.folder_root("saves");
    std::fs::create_dir_all(&dir).unwrap();
    // Locally: the first block is the peer's, the second one differs.
    let local: Vec<u8> = [first.clone(), vec![0xCC_u8; BEP_BLOCK_SIZE]].concat();
    std::fs::write(dir.join("world.bin"), &local).unwrap();

    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(quiet_control()),
        |_| {},
    )
    .expect("session failed");
    let received = peer.join();

    let content = std::fs::read(storage.resolve_safe_path("saves", "world.bin")).unwrap();
    assert!(
        content == [first, second].concat(),
        "the file was not rebuilt from its blocks"
    );
    // Only the block that differs was requested.
    let offsets: Vec<i64> = payloads(&received, BepMessageType::Request)
        .iter()
        .map(|p| syncthing_core::BepRequest::decode(p).unwrap().offset)
        .collect();
    assert_eq!(offsets, vec![BEP_BLOCK_SIZE as i64]);
}
