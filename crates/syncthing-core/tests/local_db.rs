//! Persistent folder database text format.

mod common;

use syncthing_core::local_db::{FolderDb, LocalEntry};
use syncthing_core::{BepBlockInfo, BepVersionCounter, DeviceId};

fn c(id: u64, v: u64) -> BepVersionCounter {
    BepVersionCounter {
        short_device_id: id,
        value: v,
    }
}

#[test]
fn test_db_serialization_roundtrip() {
    let mut db = FolderDb::new("f", DeviceId([7; 32]));
    db.put(LocalEntry {
        name: "dir a/save|1.bin".to_owned(),
        is_dir: false,
        deleted: false,
        size: 3,
        modified_s: 10,
        disk_mtime: 11,
        sequence: 0,
        version: vec![c(0xABCD, 42)],
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: 3,
            hash: vec![1, 2, 255],
        }],
    });
    let parsed = FolderDb::parse(&db.serialize()).expect("parse");
    assert_eq!(parsed.index_id, db.index_id);
    assert_eq!(parsed.max_sequence, 1);
    assert_eq!(parsed.entries, db.entries);
}

/// Recording downloaded files does not rewrite the whole database for each of them; the
/// pending records are saved by `flush_folder_db`.
#[test]
fn test_downloaded_files_are_saved_in_batches() {
    use syncthing_core::{BepFileInfo, BepVersionCounter, DeviceId, FsSyncStorage};
    let tmp_dir = common::fresh_temp_dir("db_batched_save");
    let storage = FsSyncStorage::new(&tmp_dir);
    let own_id = DeviceId([0x33; 32]);
    let root = storage.folder_root("f");
    std::fs::create_dir_all(&root).unwrap();
    // Loads (and saves) the fresh database.
    storage.build_local_index("f", own_id, &[]).unwrap();
    let db_file = storage.db_path("f");
    let saved = || std::fs::read_to_string(&db_file).unwrap_or_default();

    for name in ["a.bin", "b.bin"] {
        std::fs::write(root.join(name), b"").unwrap();
        let rf = BepFileInfo {
            name: name.to_owned(),
            is_dir: false,
            size: 0,
            modified_s: 1,
            deleted: false,
            invalid: false,
            sequence: 1,
            versions: vec![BepVersionCounter {
                short_device_id: 7,
                value: 1,
            }],
            blocks: Vec::new(),
        };
        storage.record_downloaded("f", own_id, &rf);
    }
    assert!(!saved().contains("a.bin"), "saved after every file");

    storage.flush_folder_db("f");
    assert!(saved().contains("a.bin") && saved().contains("b.bin"));
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
