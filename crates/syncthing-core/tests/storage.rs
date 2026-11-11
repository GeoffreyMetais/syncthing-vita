//! Block storage: streaming downloads, delta sync and PS Vita path handling.

mod common;

use std::path::{Path, PathBuf};

use syncthing_core::storage::vita_fs::with_device_root_slash;
use syncthing_core::{BepIndex, DeviceId, FsSyncStorage};

#[test]
fn test_delta_sync_and_local_index_roundtrip() {
    let tmp_dir = common::fresh_temp_dir("delta_test");
    let storage = FsSyncStorage::new(&tmp_dir);
    let dev_id = DeviceId([0x11; 32]);

    // Streaming block write
    let sample_data = b"Hello from PS Vita streaming block storage!";
    storage
        .prepare_download("test-folder", "save01.bin")
        .expect("Prepare failed");
    storage
        .write_block_streaming(
            "test-folder",
            "save01.bin",
            0,
            sample_data,
            sample_data.len() as u64,
        )
        .expect("Streaming write failed");
    storage
        .commit_download("test-folder", "save01.bin", sample_data.len() as u64)
        .expect("Commit failed");

    // Local indexing
    let (local_idx, state) = storage
        .build_local_index("test-folder", dev_id, &[])
        .expect("Local indexing failed");
    assert_eq!(local_idx.files.len(), 1);
    assert_eq!(state.max_sequence, 1);

    // Protobuf roundtrip of BepIndex
    let encoded_idx = local_idx.encode();
    let decoded_idx = BepIndex::decode(&encoded_idx).expect("BepIndex decoding failed");
    assert_eq!(decoded_idx.files.len(), 1);
    assert_eq!(decoded_idx.files[0].name, "save01.bin");

    // Delta sync check: file is already identical on disk -> 0 outdated blocks
    let outdated = storage
        .find_outdated_blocks("test-folder", &decoded_idx.files[0])
        .expect("Delta check failed");
    assert!(outdated.is_empty(), "No blocks should be re-downloaded");

    // Rescanning an unchanged folder keeps the same sequence (nothing new to advertise)
    let (_, state2) = storage
        .build_local_index("test-folder", dev_id, &[])
        .unwrap();
    assert_eq!(state2.max_sequence, state.max_sequence);
    assert_eq!(state2.index_id, state.index_id);

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_device_root_slash_variant() {
    assert_eq!(
        with_device_root_slash(Path::new("ux0:music")),
        Some(PathBuf::from("ux0:/music"))
    );
    assert_eq!(
        with_device_root_slash(Path::new("ux0:data/syncthing")),
        Some(PathBuf::from("ux0:/data/syncthing"))
    );
    assert_eq!(with_device_root_slash(Path::new("ux0:/music")), None);
    assert_eq!(with_device_root_slash(Path::new("/tmp/a:b")), None);
    assert_eq!(with_device_root_slash(Path::new("relative/dir")), None);
}
