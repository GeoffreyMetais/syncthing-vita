//! BEP session authorization: a peer only reaches folders the local configuration shares with it.

mod common;

use std::io::Cursor;
use std::sync::mpsc;
use std::time::Duration;

use common::fake_peer::{payloads, FakePeer};
use syncthing_core::session::context::SessionContext;
use syncthing_core::session::folders::{FolderSyncState, INDEX_BATCH_FILES};
use syncthing_core::session::upload::serve_block_request;
use syncthing_core::{
    connect_and_sync, connect_and_sync_with_control, read_bep_message, BepClusterConfig,
    BepFileInfo, BepHello, BepIndex, BepMessageType, BepRequest, BepResponse, DeviceId, FolderType,
    FsSyncStorage, SyncControl, SyncEvent, SyncSessionStatus, SyncTuning, VitaIdentity,
};

const LINKED_PEER: DeviceId = DeviceId([0x22; 32]);
const OTHER_PEER: DeviceId = DeviceId([0x33; 32]);
const CONTENT: &[u8] = b"save data";

/// Storage with one file in folder `saves`, shared with `LINKED_PEER` only.
fn storage_with_shared_file(name: &str) -> FsSyncStorage {
    let root = common::fresh_temp_dir(name);
    let mut storage = FsSyncStorage::new(&root);
    storage.set_folder_devices("saves", vec![LINKED_PEER]);
    let dir = storage.folder_root("saves");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("slot0.bin"), CONTENT).unwrap();
    storage
}

/// Sends a block `Request` for `folder/slot0.bin` as `peer` and returns the `Response`.
fn request_block(storage: &FsSyncStorage, peer: DeviceId, folder: &str) -> BepResponse {
    let identity = VitaIdentity::from_seed(&[7u8; 32]).unwrap();
    let mut sink = |_event: SyncEvent| {};
    let mut ctx = SessionContext::new(&identity, storage, peer, &mut sink);
    let req = BepRequest {
        id: 5,
        folder: folder.to_owned(),
        name: "slot0.bin".to_owned(),
        offset: 0,
        size: CONTENT.len() as i32,
        hash: Vec::new(),
    };
    let mut wire = Vec::new();
    serve_block_request(&mut wire, &mut ctx, &req.encode()).unwrap();
    let (msg_type, payload) = read_bep_message(&mut Cursor::new(wire)).unwrap();
    assert_eq!(msg_type, BepMessageType::Response);
    BepResponse::decode(&payload).unwrap()
}

#[test]
fn test_folder_sharing_is_per_device() {
    let storage = storage_with_shared_file("session_share_gate");
    assert!(storage.is_folder_shared_with("saves", &LINKED_PEER));
    assert!(!storage.is_folder_shared_with("saves", &OTHER_PEER));
    assert!(!storage.is_folder_shared_with("unknown", &LINKED_PEER));
}

#[test]
fn test_block_is_served_to_linked_peer() {
    let storage = storage_with_shared_file("session_serve_linked");
    let resp = request_block(&storage, LINKED_PEER, "saves");
    assert_eq!(resp.code, 0);
    assert_eq!(resp.data, CONTENT);
}

#[test]
fn test_block_is_refused_to_unlinked_peer() {
    let storage = storage_with_shared_file("session_serve_unlinked");
    let resp = request_block(&storage, OTHER_PEER, "saves");
    assert_ne!(resp.code, 0);
    assert!(resp.data.is_empty());
}

#[test]
fn test_block_is_refused_for_unconfigured_folder() {
    let storage = storage_with_shared_file("session_serve_unknown");
    // The file exists on disk under the default root, but the folder is not a configured share.
    let dir = storage.folder_root("stray");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("slot0.bin"), CONTENT).unwrap();
    let resp = request_block(&storage, LINKED_PEER, "stray");
    assert_ne!(resp.code, 0);
    assert!(resp.data.is_empty());
}

#[test]
fn test_block_is_refused_for_paused_folder() {
    let mut storage = storage_with_shared_file("session_serve_paused");
    storage.set_folder_paused("saves", true);
    let resp = request_block(&storage, LINKED_PEER, "saves");
    assert_ne!(resp.code, 0);
    assert!(resp.data.is_empty());
}

/// Identity of the device under test in end-to-end sessions.
fn vita_identity() -> VitaIdentity {
    VitaIdentity::from_seed(&[7u8; 32]).unwrap()
}

/// Runs one single-cycle session against `peer` and returns the emitted events.
fn sync_once(peer: &FakePeer, storage: &FsSyncStorage) -> (SyncSessionStatus, Vec<SyncEvent>) {
    let mut events = Vec::new();
    let result = connect_and_sync(
        &vita_identity(),
        peer.addr,
        Some(peer.device_id),
        &BepHello::new_vita_default(),
        storage,
        Duration::from_secs(4),
        |event| events.push(event),
    )
    .expect("session failed");
    (result.session_status, events)
}

#[test]
fn test_session_only_syncs_folders_shared_with_the_peer() {
    let vita_id = vita_identity().device_id;
    let peer = FakePeer::spawn(0x51, move |conn| {
        let shared = conn.file("shared", "a.txt", b"shared content", 1);
        let secret = conn.file("secret", "s.txt", b"never accepted", 1);
        conn.send_cluster_config(&["shared", "secret"], vita_id);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", vec![shared], false);
        conn.send_index("secret", vec![secret], false);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let root = common::fresh_temp_dir("session_e2e_authorization");
    let mut storage = FsSyncStorage::new(&root);
    storage.set_folder_devices("shared", vec![peer.device_id]);

    let (status, events) = sync_once(&peer, &storage);
    assert!(matches!(status, SyncSessionStatus::Active { .. }));
    let received = peer.join();

    // The accepted folder was pulled; the other one was neither written nor requested.
    let pulled = storage.resolve_safe_path("shared", "a.txt");
    assert_eq!(std::fs::read(pulled).unwrap(), b"shared content");
    assert!(!storage.folder_root("secret").exists());
    for payload in payloads(&received, BepMessageType::Request) {
        assert_eq!(BepRequest::decode(payload).unwrap().folder, "shared");
    }

    // Our `ClusterConfig` and `Index` only mention the accepted folder...
    let configs = payloads(&received, BepMessageType::ClusterConfig);
    let announced: Vec<String> = BepClusterConfig::decode(configs[0])
        .unwrap()
        .folders
        .into_iter()
        .map(|f| f.id)
        .collect();
    assert_eq!(announced, vec!["shared".to_owned()]);
    for payload in payloads(&received, BepMessageType::Index) {
        assert_eq!(BepIndex::decode(payload).unwrap().folder, "shared");
    }

    // ...while the application still learns about both, to ask for consent.
    let offered = events.iter().find_map(|e| match e {
        SyncEvent::ClusterConfigReceived { folders, .. } => Some(folders.len()),
        _ => None,
    });
    assert_eq!(offered, Some(2));
}

#[test]
fn test_index_update_received_during_a_download_is_applied() {
    let vita_id = vita_identity().device_id;
    let peer = FakePeer::spawn(0x52, move |conn| {
        let first = conn.file("shared", "a.txt", b"first file", 1);
        let late = conn.file("shared", "b.txt", b"announced mid-download", 2);
        conn.send_cluster_config(&["shared"], vita_id);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", vec![first], false);
        // The update arrives while the device waits for the block of `a.txt`.
        let pending = conn
            .wait_for(BepMessageType::Request, Duration::from_secs(5))
            .expect("block request");
        conn.send_index("shared", vec![late], true);
        conn.answer(&pending);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let root = common::fresh_temp_dir("session_e2e_deferred_index");
    let mut storage = FsSyncStorage::new(&root);
    storage.set_folder_devices("shared", vec![peer.device_id]);

    sync_once(&peer, &storage);
    peer.join();

    let read = |name: &str| std::fs::read(storage.resolve_safe_path("shared", name)).unwrap();
    assert_eq!(read("a.txt"), b"first file");
    assert_eq!(read("b.txt"), b"announced mid-download");
}

#[test]
fn test_local_change_is_published_as_an_incremental_index_update() {
    let vita_id = vita_identity().device_id;
    let peer = FakePeer::spawn(0x53, move |conn| {
        let remote = conn.file("shared", "remote.txt", b"from the peer", 1);
        conn.send_cluster_config(&["shared"], vita_id);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", vec![remote], false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(10))
            .expect("full Index");
        conn.wait_for(BepMessageType::IndexUpdate, Duration::from_secs(15))
            .expect("IndexUpdate after the local change");
    });

    let root = common::fresh_temp_dir("session_e2e_incremental_index");
    let mut storage = FsSyncStorage::new(&root);
    storage.set_folder_devices("shared", vec![peer.device_id]);
    let local_path = storage.resolve_safe_path("shared", "local.txt");

    // A long rescan interval: only the manual rescan below can publish the change.
    let control = SyncControl::with_tuning(SyncTuning {
        rescan_interval: Duration::from_secs(3600),
        ..SyncTuning::default()
    });
    let (events_tx, events_rx) = mpsc::channel();
    let (peer_addr, peer_id) = (peer.addr, peer.device_id);
    let session_control = std::sync::Arc::clone(&control);
    let session = std::thread::spawn(move || {
        connect_and_sync_with_control(
            &vita_identity(),
            peer_addr,
            Some(peer_id),
            &BepHello::new_vita_default(),
            &storage,
            Duration::from_secs(4),
            Some(session_control),
            move |event| {
                let _ = events_tx.send(event);
            },
        )
    });

    // Once the initial cycle is over, change the folder locally and ask for a rescan.
    loop {
        let event = events_rx
            .recv_timeout(Duration::from_secs(20))
            .expect("initial sync cycle");
        if matches!(event, SyncEvent::SyncCycleCompleted { .. }) {
            break;
        }
    }
    std::fs::write(&local_path, b"created on the Vita").unwrap();
    control.request_rescan();

    let received = peer.join();
    control.request_stop();
    session.join().unwrap().expect("session failed");

    let names = |payload: &[u8]| -> Vec<String> {
        let index = BepIndex::decode(payload).unwrap();
        index.files.into_iter().map(|f| f.name).collect()
    };
    // The full `Index` holds the downloaded file...
    let full = payloads(&received, BepMessageType::Index);
    assert_eq!(full.len(), 1);
    assert_eq!(names(full[0]), vec!["remote.txt".to_owned()]);
    // ...and the update only the new one, not the whole folder again.
    let updates = payloads(&received, BepMessageType::IndexUpdate);
    assert_eq!(
        names(updates[updates.len() - 1]),
        vec!["local.txt".to_owned()]
    );
}

fn indexed_file(name: String, sequence: i64) -> BepFileInfo {
    BepFileInfo {
        name,
        is_dir: false,
        size: 0,
        modified_s: 0,
        deleted: false,
        invalid: false,
        sequence,
        versions: Vec::new(),
        blocks: Vec::new(),
    }
}

#[test]
fn test_index_publication_is_batched_and_incremental() {
    let count = 2 * INDEX_BATCH_FILES + 500;
    let mut index = BepIndex {
        folder: "f".to_owned(),
        files: (1..=count as i64)
            .map(|seq| indexed_file(format!("file{seq}"), seq))
            .collect(),
    };
    let mut state = FolderSyncState::default();

    // First publication: a full `Index`, continued by `IndexUpdate` batches.
    let first = state.take_unpublished(&index);
    let shape: Vec<(BepMessageType, usize)> =
        first.iter().map(|(t, i)| (*t, i.files.len())).collect();
    assert_eq!(
        shape,
        vec![
            (BepMessageType::Index, INDEX_BATCH_FILES),
            (BepMessageType::IndexUpdate, INDEX_BATCH_FILES),
            (BepMessageType::IndexUpdate, 500),
        ]
    );

    // Nothing changed: nothing to send.
    assert!(state.take_unpublished(&index).is_empty());

    // One entry changed (new sequence): only that entry is sent.
    index.files[3].sequence = count as i64 + 1;
    let update = state.take_unpublished(&index);
    assert_eq!(update.len(), 1);
    assert_eq!(update[0].0, BepMessageType::IndexUpdate);
    assert_eq!(update[0].1.files.len(), 1);
    assert_eq!(update[0].1.files[0].name, "file4");

    // An empty folder still announces itself once with an empty full `Index`.
    let empty = BepIndex {
        folder: "empty".to_owned(),
        files: Vec::new(),
    };
    let announced = state.take_unpublished(&empty);
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].0, BepMessageType::Index);
    assert!(state.take_unpublished(&empty).is_empty());
}

#[test]
fn test_scan_reports_progress_for_keepalives() {
    let storage = storage_with_shared_file("session_scan_progress");
    let mut ticks = 0;
    storage
        .build_local_index_with_progress("saves", LINKED_PEER, &[], &mut || ticks += 1)
        .unwrap();
    assert!(ticks >= 2, "one tick per entry and per hashed block");

    // A session with a zero keepalive interval is always due for a `Ping`.
    let identity = vita_identity();
    let mut sink = |_event: SyncEvent| {};
    let mut ctx = SessionContext::new(&identity, &storage, LINKED_PEER, &mut sink);
    assert!(!ctx.keepalive_due());
    ctx.tuning.keepalive_interval = Duration::ZERO;
    assert!(ctx.keepalive_due());
}

/// Wire value of the `type` field (3) of each folder of a `ClusterConfig` payload.
fn announced_folder_types(payload: &[u8]) -> Vec<(String, u64)> {
    use syncthing_core::proto::{decode_varint, WireIter};
    let mut out = Vec::new();
    for item in WireIter::new(payload) {
        let (field, _, folder) = item.unwrap();
        if field != 1 {
            continue;
        }
        let (mut id, mut folder_type) = (String::new(), 0);
        for f in WireIter::new(folder) {
            match f.unwrap() {
                (1, 2, data) => id = String::from_utf8_lossy(data).into_owned(),
                (3, 0, data) => folder_type = decode_varint(data).unwrap().0,
                _ => {}
            }
        }
        out.push((id, folder_type));
    }
    out
}

#[test]
fn test_receive_only_folder_applies_remote_changes_but_never_offers_local_ones() {
    let vita_id = vita_identity().device_id;
    let peer = FakePeer::spawn(0x55, move |conn| {
        let remote = conn.file("saves", "remote.txt", b"from the peer", 1);
        conn.send_cluster_config(&["saves"], vita_id);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("saves", vec![remote], false);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let root = common::fresh_temp_dir("session_e2e_receive_only");
    let mut storage = FsSyncStorage::new(&root);
    storage.set_folder_devices("saves", vec![peer.device_id]);
    storage.set_folder_type("saves", FolderType::ReceiveOnly);
    std::fs::create_dir_all(storage.folder_root("saves")).unwrap();
    std::fs::write(
        storage.resolve_safe_path("saves", "local.txt"),
        b"vita save",
    )
    .unwrap();

    sync_once(&peer, &storage);
    let received = peer.join();

    // The peer's file was pulled...
    let pulled = storage.resolve_safe_path("saves", "remote.txt");
    assert_eq!(std::fs::read(pulled).unwrap(), b"from the peer");
    // ...while the local-only file is announced invalid, so the peer never pulls it.
    let published: Vec<(String, bool)> = payloads(&received, BepMessageType::Index)
        .iter()
        .chain(payloads(&received, BepMessageType::IndexUpdate).iter())
        .flat_map(|p| BepIndex::decode(p).unwrap().files)
        .map(|f| (f.name, f.invalid))
        .collect();
    assert!(published.contains(&("local.txt".to_owned(), true)));
    assert!(published.contains(&("remote.txt".to_owned(), false)));
    assert!(!published.contains(&("local.txt".to_owned(), false)));
    // The folder is announced as receive-only.
    let configs = payloads(&received, BepMessageType::ClusterConfig);
    assert_eq!(
        announced_folder_types(configs[0]),
        vec![("saves".to_owned(), FolderType::ReceiveOnly.wire_value())]
    );
}

#[test]
fn test_receive_only_folder_still_serves_blocks() {
    let mut storage = storage_with_shared_file("session_receive_only_serves");
    storage.set_folder_type("saves", FolderType::ReceiveOnly);
    let resp = request_block(&storage, LINKED_PEER, "saves");
    assert_eq!(resp.code, 0);
    assert_eq!(resp.data, CONTENT);
}
