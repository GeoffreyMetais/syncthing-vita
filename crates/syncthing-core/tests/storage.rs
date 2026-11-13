//! Block storage: streaming downloads, delta sync and PS Vita path handling.

mod common;

use std::path::{Path, PathBuf};

use syncthing_core::storage::vita_fs::with_device_root_slash;
use syncthing_core::{BepIndex, DeviceId, FolderType, FsSyncStorage};

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

#[test]
fn test_peer_supplied_block_sizes_are_bounded() {
    use syncthing_core::{BepBlockInfo, BepFileInfo, BepVersionCounter, BEP_MAX_BLOCK_SIZE};

    let tmp_dir = common::fresh_temp_dir("block_size_bounds");
    let storage = FsSyncStorage::new(&tmp_dir);
    let dir = storage.folder_root("f");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.bin"), b"data").unwrap();

    // A block request larger than the BEP maximum is refused before any allocation.
    assert!(storage
        .read_block_for_peer("f", "a.bin", 0, BEP_MAX_BLOCK_SIZE + 1)
        .is_err());
    assert_eq!(
        storage.read_block_for_peer("f", "a.bin", 0, 4).unwrap(),
        b"data"
    );

    // So is an index announcing an oversized or negative block.
    for size in [i32::MAX, -1] {
        let remote = BepFileInfo {
            name: "a.bin".to_owned(),
            is_dir: false,
            size: 4,
            modified_s: 1,
            deleted: false,
            invalid: false,
            sequence: 1,
            versions: vec![BepVersionCounter {
                short_device_id: 1,
                value: 1,
            }],
            blocks: vec![BepBlockInfo {
                offset: 0,
                size,
                hash: vec![0; 32],
            }],
        };
        assert!(storage.find_outdated_blocks("f", &remote).is_err());
    }
}

#[test]
fn test_interrupted_commit_restores_the_previous_file() {
    let tmp_dir = common::fresh_temp_dir("commit_recovery");
    let storage = FsSyncStorage::new(&tmp_dir);
    let dev_id = DeviceId([0x11; 32]);
    let dir = storage.folder_root("f");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("save.bin"), b"version 1").unwrap();
    let (index, _) = storage.build_local_index("f", dev_id, &[]).unwrap();
    assert!(index
        .files
        .iter()
        .any(|f| f.name == "save.bin" && !f.deleted));

    // State left by a commit interrupted after the old file was set aside.
    std::fs::rename(
        dir.join("save.bin"),
        storage.download_backup_path("f", "save.bin"),
    )
    .unwrap();

    let (index, _) = storage.build_local_index("f", dev_id, &[]).unwrap();
    assert!(index
        .files
        .iter()
        .any(|f| f.name == "save.bin" && !f.deleted));
    assert_eq!(std::fs::read(dir.join("save.bin")).unwrap(), b"version 1");
    assert!(!storage.download_backup_path("f", "save.bin").exists());
}

#[test]
fn test_folder_type_defaults_to_send_receive() {
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("storage_folder_type"));
    assert_eq!(storage.folder_type("saves"), FolderType::SendReceive);
    assert!(!storage.is_folder_send_only("saves"));

    storage.set_folder_type("saves", FolderType::SendOnly);
    assert_eq!(storage.folder_type("saves"), FolderType::SendOnly);
    assert!(storage.is_folder_send_only("saves"));
    assert!(!storage.is_folder_send_only("music"));

    storage.set_folder_type("saves", FolderType::SendReceive);
    assert!(!storage.is_folder_send_only("saves"));
}
