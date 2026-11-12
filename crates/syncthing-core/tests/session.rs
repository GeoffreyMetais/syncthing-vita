//! BEP session authorization: a peer only reaches folders the local configuration shares with it.

mod common;

use std::io::Cursor;

use syncthing_core::session::context::SessionContext;
use syncthing_core::session::upload::serve_block_request;
use syncthing_core::{
    read_bep_message, BepMessageType, BepRequest, BepResponse, DeviceId, FsSyncStorage, SyncEvent,
    VitaIdentity,
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
