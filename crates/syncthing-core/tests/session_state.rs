//! Session building blocks tested directly: the control handle shared with the UI, the
//! per-session context, the folder bookkeeping and the upload statistics.

mod common;

use std::time::Duration;

use syncthing_core::session::context::SessionContext;
use syncthing_core::session::control::SyncControl;
use syncthing_core::session::folders::{published_file_count, FolderSyncState};
use syncthing_core::session::upload::serve_block_request;
use syncthing_core::{
    BepFileInfo, BepFolder, BepHello, BepIndex, BepRequest, DeviceId, DeviceIdentity,
    FsSyncStorage, SyncEvent, SyncTuning,
};

const PEER: DeviceId = DeviceId([0x44; 32]);

fn tuning(rescan_secs: u64, keepalive_ms: u64) -> SyncTuning {
    SyncTuning {
        rescan_interval: Duration::from_secs(rescan_secs),
        keepalive_interval: Duration::from_millis(keepalive_ms),
    }
}

#[test]
fn test_control_keeps_the_tuning_it_was_created_with() {
    let custom = tuning(7, 250);
    let control = SyncControl::with_tuning(custom);
    assert_eq!(control.tuning(), custom);
    assert_ne!(control.tuning(), SyncTuning::default());
    assert_eq!(SyncControl::new().tuning(), SyncTuning::default());
}

#[test]
fn test_rescan_request_is_taken_once_and_counted_by_epoch() {
    let control = SyncControl::new();
    assert!(!control.take_rescan_request());
    assert_eq!(control.rescan_epoch(), 0);

    control.request_rescan();
    assert_eq!(control.rescan_epoch(), 1);
    assert!(control.take_rescan_request());
    assert!(!control.take_rescan_request());
    // The epoch counts requests; taking the request does not change it.
    assert_eq!(control.rescan_epoch(), 1);

    control.request_rescan();
    control.request_rescan();
    assert_eq!(control.rescan_epoch(), 3);
    assert!(control.take_rescan_request());
}

#[test]
fn test_context_attaches_the_control_handle_timings_and_stop_flag() {
    let identity = DeviceIdentity::from_seed(&[8u8; 32]).unwrap();
    let hello = BepHello::new("Test device", "syncthing-core-tests", "0.0.0");
    let root = common::fresh_temp_dir("session_state_context");
    let storage = FsSyncStorage::new(&root);
    let mut sink = |_event: SyncEvent| {};
    let mut ctx = SessionContext::new(&identity, &hello, &storage, PEER, &mut sink);

    let control = SyncControl::with_tuning(tuning(3600, 0));
    assert!(!ctx.is_stop_requested());
    ctx.set_control(&control);
    assert_eq!(ctx.tuning, control.tuning());
    // A zero keepalive interval makes a keepalive due immediately.
    assert!(ctx.keepalive_due());

    assert!(!ctx.is_stop_requested());
    control.request_stop();
    assert!(ctx.is_stop_requested());
}

#[test]
fn test_context_without_control_keeps_the_default_timings() {
    let identity = DeviceIdentity::from_seed(&[8u8; 32]).unwrap();
    let hello = BepHello::new("Test device", "syncthing-core-tests", "0.0.0");
    let storage = FsSyncStorage::new(common::fresh_temp_dir("session_state_default"));
    let mut sink = |_event: SyncEvent| {};
    let mut ctx = SessionContext::new(&identity, &hello, &storage, PEER, &mut sink);
    assert_eq!(ctx.tuning, SyncTuning::default());
    assert!(!ctx.is_stop_requested());
    assert!(!ctx.keepalive_due());
}

#[test]
fn test_request_ids_are_positive_and_increase() {
    let identity = DeviceIdentity::from_seed(&[8u8; 32]).unwrap();
    let hello = BepHello::new("Test device", "syncthing-core-tests", "0.0.0");
    let storage = FsSyncStorage::new(common::fresh_temp_dir("session_state_request_ids"));
    let mut sink = |_event: SyncEvent| {};
    let mut ctx = SessionContext::new(&identity, &hello, &storage, PEER, &mut sink);
    let ids: Vec<i32> = (0..4).map(|_| ctx.next_request_id()).collect();
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn test_served_blocks_are_counted_in_the_upload_statistics() {
    let identity = DeviceIdentity::from_seed(&[8u8; 32]).unwrap();
    let hello = BepHello::new("Test device", "syncthing-core-tests", "0.0.0");
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("session_state_upload"));
    storage.set_folder_devices("saves", vec![PEER]);
    let dir = storage.folder_root("saves");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("slot.bin"), b"0123456789").unwrap();

    let mut sink = |_event: SyncEvent| {};
    let mut ctx = SessionContext::new(&identity, &hello, &storage, PEER, &mut sink);
    let request = |offset: i64, size: i32| BepRequest {
        id: 9,
        folder: "saves".to_owned(),
        name: "slot.bin".to_owned(),
        offset,
        size,
        hash: Vec::new(),
    };
    let mut wire = Vec::new();
    serve_block_request(&mut wire, &mut ctx, &request(0, 4).encode()).unwrap();
    serve_block_request(&mut wire, &mut ctx, &request(4, 6).encode()).unwrap();
    assert_eq!(ctx.stats.blocks_uploaded, 2);
    assert_eq!(ctx.stats.bytes_uploaded, 10);

    // A refused request is answered but not counted.
    let mut storage_refused =
        FsSyncStorage::new(common::fresh_temp_dir("session_state_upload_refused"));
    storage_refused.set_folder_devices("saves", Vec::new());
    let mut sink = |_event: SyncEvent| {};
    let mut refused_ctx = SessionContext::new(&identity, &hello, &storage_refused, PEER, &mut sink);
    serve_block_request(&mut wire, &mut refused_ctx, &request(0, 4).encode()).unwrap();
    assert_eq!(refused_ctx.stats.blocks_uploaded, 0);
    assert_eq!(refused_ctx.stats.bytes_uploaded, 0);
}

fn file(name: &str, sequence: i64, is_dir: bool, deleted: bool) -> BepFileInfo {
    BepFileInfo {
        name: name.to_owned(),
        is_dir,
        size: 0,
        modified_s: 0,
        deleted,
        invalid: false,
        sequence,
        versions: Vec::new(),
        blocks: Vec::new(),
    }
}

#[test]
fn test_published_file_count_ignores_directories_and_deletions() {
    let empty = BepIndex {
        folder: "f".to_owned(),
        files: Vec::new(),
    };
    assert_eq!(published_file_count(&empty), 0);

    let index = BepIndex {
        folder: "f".to_owned(),
        files: vec![
            file("a.txt", 1, false, false),
            file("b.txt", 2, false, false),
            file("dir", 3, true, false),
            file("gone.txt", 4, false, true),
            file("gone-dir", 5, true, true),
        ],
    };
    assert_eq!(published_file_count(&index), 2);
}

#[test]
fn test_local_indexes_are_stored_and_looked_up_per_folder() {
    let mut folders = FolderSyncState::default();
    assert!(folders.local_index("a").is_none());

    let index = |folder: &str, files: usize| BepIndex {
        folder: folder.to_owned(),
        files: (0..files)
            .map(|i| file(&format!("{folder}{i}"), i as i64 + 1, false, false))
            .collect(),
    };
    folders.upsert_local_index(index("a", 1));
    folders.upsert_local_index(index("b", 2));
    assert_eq!(folders.local_index("a").unwrap().files.len(), 1);
    assert_eq!(folders.local_index("b").unwrap().files.len(), 2);
    assert!(folders.local_index("c").is_none());

    // Updating one folder replaces its index in place and leaves the other ones alone.
    folders.upsert_local_index(index("a", 3));
    assert_eq!(folders.local_indexes.len(), 2);
    assert_eq!(folders.local_index("a").unwrap().files.len(), 3);
    assert_eq!(folders.local_index("b").unwrap().files.len(), 2);
}

#[test]
fn test_shared_and_peer_folders_are_looked_up_by_id() {
    let mut folders = FolderSyncState::default();
    folders.shared_folders.push(BepFolder {
        id: "saves".to_owned(),
        label: "saves".to_owned(),
        device_ids: Vec::new(),
    });
    folders.peer_folder_ids.push("music".to_owned());
    assert!(folders.is_shared_folder("saves"));
    assert!(!folders.is_shared_folder("music"));
    assert!(folders.is_peer_folder("music"));
    assert!(!folders.is_peer_folder("saves"));
}
