//! Persistent folder database text format.

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
