//! Exact on-disk text format of the folder database (`#vita-db-v1`) and tolerance of malformed
//! lines: a damaged line is skipped or loses only the damaged field, never the whole database.

use syncthing_core::local_db::{FolderDb, LocalEntry};
use syncthing_core::{BepBlockInfo, BepVersionCounter, DeviceId};

fn c(id: u64, v: u64) -> BepVersionCounter {
    BepVersionCounter {
        short_device_id: id,
        value: v,
    }
}

fn entry(name: &str, is_dir: bool, deleted: bool) -> LocalEntry {
    LocalEntry {
        name: name.to_owned(),
        is_dir,
        deleted,
        size: 0,
        modified_s: 0,
        disk_mtime: 0,
        sequence: 0,
        version: Vec::new(),
        blocks: Vec::new(),
    }
}

#[test]
fn test_serialized_text_has_the_exact_format() {
    let mut db = FolderDb::new("f", DeviceId([1; 32]));
    db.index_id = 0xabc;
    db.put(LocalEntry {
        size: 3,
        modified_s: 10,
        disk_mtime: 11,
        version: vec![c(0x1f, 42)],
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: 3,
            hash: vec![0x01, 0xff],
        }],
        ..entry("a|b", false, false)
    });
    db.put(LocalEntry {
        modified_s: 5,
        disk_mtime: 6,
        ..entry("dir", true, false)
    });
    db.put(LocalEntry {
        size: 0,
        modified_s: 7,
        version: vec![c(2, 1), c(0xa, 3)],
        ..entry("gone.txt", false, true)
    });

    let expected = concat!(
        "#vita-db-v1\tabc\t3\n",
        "F\t0\t3\t10\t11\t1\t1f:42\t0:3:01ff\ta|b\n",
        "D\t0\t0\t5\t6\t2\t\t\tdir\n",
        "F\t1\t0\t7\t0\t3\t2:1,a:3\t\tgone.txt\n",
    );
    assert_eq!(db.serialize(), expected);

    let parsed = FolderDb::parse(expected).expect("parse");
    assert_eq!(parsed.index_id, 0xabc);
    assert_eq!(parsed.max_sequence, 3);
    assert_eq!(parsed.entries, db.entries);
    assert_eq!(parsed.serialize(), expected, "serialization is stable");
}

#[test]
fn test_header_must_be_the_exact_magic_with_three_fields() {
    assert!(FolderDb::parse("").is_none());
    assert!(FolderDb::parse("#vita-db-v2\t1\t0\n").is_none());
    assert!(FolderDb::parse("#vita-db-x\t1\t0\n").is_none());
    assert!(FolderDb::parse("#vita-db-v1\t1\n").is_none());
    assert!(FolderDb::parse("#vita-db-v1\t1\t0\textra\n").is_none());
    assert!(
        FolderDb::parse("#vita-db-v1\tzz\t0\n").is_none(),
        "index id is hex"
    );
    assert!(
        FolderDb::parse("#vita-db-v1\t1\tx\n").is_none(),
        "sequence is decimal"
    );
    let db = FolderDb::parse("#vita-db-v1\t1\t0\n").expect("empty database");
    assert_eq!((db.index_id, db.max_sequence), (1, 0));
    assert!(db.entries.is_empty());
}

/// Lines with the wrong number of fields and lines without a name are skipped; the lines around
/// them are kept.
#[test]
fn test_lines_with_a_wrong_field_count_or_no_name_are_skipped() {
    let text = concat!(
        "#vita-db-v1\t1\t3\n",
        "F\t0\t1\t1\t1\t1\t\t\tgood.txt\n",
        "F\t0\t1\t1\t1\t2\t\tno-name-field\n",
        "F\t0\t1\t1\t1\t2\t\t\t\n",
        "F\t0\t1\t1\t1\t2\t\t\tafter.txt\textra\n",
        "\n",
        "F\t0\t1\t1\t1\t3\t\t\tlast.txt\n",
    );
    let db = FolderDb::parse(text).expect("parse");
    let mut names: Vec<&str> = db.entries.keys().map(String::as_str).collect();
    names.sort_unstable();
    // The name is the last field: extra tabs stay part of it.
    assert_eq!(names, vec!["after.txt\textra", "good.txt", "last.txt"]);
    assert_eq!(db.max_sequence, 3);
}

/// Damaged version counters and block descriptions are dropped one by one; the rest of the line
/// (the entry itself) is kept.
#[test]
fn test_damaged_versions_and_blocks_are_dropped_individually() {
    let text = concat!(
        "#vita-db-v1\t1\t1\n",
        "F\t0\t4\t1\t1\t1\t1f:42,zz:1,nocolon,2:x,a:7\t",
        "0:4:01ff,bad,1:2:zz,4:4:abc,8:0:\tdamaged.bin\n",
    );
    let db = FolderDb::parse(text).expect("parse");
    let e = &db.entries["damaged.bin"];
    assert_eq!(e.version, vec![c(0x1f, 42), c(0xa, 7)]);
    assert_eq!(
        e.blocks,
        vec![
            BepBlockInfo {
                offset: 0,
                size: 4,
                hash: vec![0x01, 0xff],
            },
            BepBlockInfo {
                offset: 8,
                size: 0,
                hash: Vec::new(),
            },
        ]
    );
    assert_eq!(e.size, 4);
}

#[test]
fn test_flags_and_numbers_fall_back_to_safe_values() {
    // Only the flag `1` marks a tombstone and only `D` a directory; bad numbers become zero.
    let text = concat!(
        "#vita-db-v1\t1\t0\n",
        "D\t1\tx\ty\tz\tq\t\t\tdir-tomb\n",
        "F\t7\t2\t3\t4\t5\t\t\tfile-odd-flag\n",
    );
    let db = FolderDb::parse(text).expect("parse");
    let dir = &db.entries["dir-tomb"];
    assert!(dir.is_dir && dir.deleted);
    assert_eq!(
        (dir.size, dir.modified_s, dir.disk_mtime, dir.sequence),
        (0, 0, 0, 0)
    );
    let file = &db.entries["file-odd-flag"];
    assert!(!file.is_dir && !file.deleted);
    assert_eq!(
        (file.size, file.modified_s, file.disk_mtime, file.sequence),
        (2, 3, 4, 5)
    );
    assert_eq!(db.max_sequence, 5, "the highest stored sequence is kept");
}

#[test]
fn test_hex_fields_reject_odd_length_and_non_hex_text() {
    let text = concat!(
        "#vita-db-v1\t1\t0\n",
        "F\t0\t1\t1\t1\t1\t\t0:1:abc,0:1:zz,0:1:0a\tblocks.bin\n",
    );
    let db = FolderDb::parse(text).expect("parse");
    assert_eq!(
        db.entries["blocks.bin"].blocks,
        vec![BepBlockInfo {
            offset: 0,
            size: 1,
            hash: vec![0x0a],
        }]
    );
}
