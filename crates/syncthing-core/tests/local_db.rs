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
            weak_hash: 0,
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

fn entry(
    name: &str,
    is_dir: bool,
    deleted: bool,
    size: i64,
    blocks: Vec<BepBlockInfo>,
) -> LocalEntry {
    LocalEntry {
        name: name.to_owned(),
        is_dir,
        deleted,
        size,
        modified_s: 5,
        disk_mtime: 6,
        sequence: 0,
        version: vec![c(1, 1)],
        blocks,
    }
}

fn block(offset: i64, size: i32, hash: u8) -> BepBlockInfo {
    BepBlockInfo {
        offset,
        size,
        hash: vec![hash; 32],
        weak_hash: 0,
    }
}

/// The wire view of an entry: tombstones and directories carry no size and no blocks.
#[test]
fn test_file_info_drops_size_and_blocks_of_tombstones_and_directories() {
    let live = entry("live.bin", false, false, 4, vec![block(0, 4, 1)]);
    let info = live.to_file_info();
    assert_eq!((info.size, info.blocks.len(), info.deleted), (4, 1, false));

    let tombstone = entry("gone.bin", false, true, 4, vec![block(0, 4, 1)]);
    let info = tombstone.to_file_info();
    assert_eq!(info.size, 0);
    assert!(info.blocks.is_empty());
    assert!(info.deleted && !info.is_dir && !info.invalid);

    let dir = entry("dir", true, false, 9, vec![block(0, 9, 1)]);
    let info = dir.to_file_info();
    assert!(info.is_dir);
    assert_eq!(info.size, 0);
    assert!(info.blocks.is_empty());
    assert_eq!((info.modified_s, info.sequence), (5, 0));
}

/// Same content means the same type, deletion state, size and block hashes; a block of the
/// same hash but another size, or a missing block, is another content.
#[test]
fn test_content_equals_compares_type_state_size_and_blocks() {
    use syncthing_core::BepFileInfo;
    let info = |is_dir: bool, deleted: bool, size: i64, blocks: Vec<BepBlockInfo>| BepFileInfo {
        name: "x".to_owned(),
        is_dir,
        size,
        modified_s: 1,
        deleted,
        invalid: false,
        sequence: 1,
        versions: Vec::new(),
        blocks,
    };
    let file = entry("x", false, false, 4, vec![block(0, 4, 1)]);
    assert!(file.content_equals(&info(false, false, 4, vec![block(0, 4, 1)])));

    // Deletion state: a tombstone only matches a tombstone.
    let tomb = entry("x", false, true, 0, Vec::new());
    assert!(!file.content_equals(&info(false, true, 0, Vec::new())));
    assert!(!tomb.content_equals(&info(false, false, 0, Vec::new())));
    assert!(tomb.content_equals(&info(false, true, 0, Vec::new())));

    // Type: a directory never matches a file, even with equal fields.
    let dir = entry("x", true, false, 0, Vec::new());
    assert!(!dir.content_equals(&info(false, false, 0, Vec::new())));
    assert!(!file.content_equals(&info(true, false, 0, Vec::new())));
    assert!(dir.content_equals(&info(true, false, 0, Vec::new())));

    // Size, block count, block size and block hash each decide on their own.
    assert!(!file.content_equals(&info(false, false, 5, vec![block(0, 4, 1)])));
    let two = entry("x", false, false, 4, vec![block(0, 2, 1), block(2, 2, 2)]);
    assert!(!two.content_equals(&info(false, false, 4, vec![block(0, 2, 1)])));
    assert!(!file.content_equals(&info(false, false, 4, vec![block(0, 2, 1)])));
    assert!(!file.content_equals(&info(false, false, 4, vec![block(0, 4, 2)])));
}

/// Touching the disk time records it without a new sequence number; unknown names are ignored.
#[test]
fn test_touch_disk_mtime_keeps_the_sequence() {
    let mut db = FolderDb::new("f", DeviceId([7; 32]));
    db.put(entry("a.txt", false, false, 1, vec![block(0, 1, 1)]));
    let before = db.entries["a.txt"].clone();
    db.touch_disk_mtime("a.txt", 99);
    db.touch_disk_mtime("missing.txt", 99);
    let after = &db.entries["a.txt"];
    assert_eq!(after.disk_mtime, 99);
    assert_eq!(after.sequence, before.sequence);
    assert_eq!(after.version, before.version);
    assert_eq!(db.max_sequence, 1);
    assert!(!db.entries.contains_key("missing.txt"));
}
