//! Local disk scans into the folder database: change detection (size, mtime, type), tombstones,
//! the missing-folder guard, adoption of identical remote versions and unusual file kinds.

mod common;

use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use syncthing_core::local_db::FolderDb;
use syncthing_core::version_vector::{short_device_id, version_ge};
use syncthing_core::{BepBlockInfo, BepFileInfo, BepVersionCounter, DeviceId, FsSyncStorage};

const OWN: DeviceId = DeviceId([0x55; 32]);
const T: u64 = 1_600_000_000;

fn set_mtime(path: &Path, secs: u64) {
    File::open(path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn stored_db(storage: &FsSyncStorage) -> FolderDb {
    FolderDb::parse(&fs::read_to_string(storage.db_path("f")).unwrap()).unwrap()
}

fn scan(storage: &FsSyncStorage) -> syncthing_core::BepIndex {
    storage.build_local_index("f", OWN, &[]).unwrap().0
}

fn find<'a>(index: &'a syncthing_core::BepIndex, name: &str) -> &'a syncthing_core::BepFileInfo {
    index
        .files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} not in index"))
}

#[test]
fn test_scanned_modification_times_are_advertised() {
    let dir = common::fresh_temp_dir("scan_mtime");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("m.txt"), b"m").unwrap();
    set_mtime(&root.join("m.txt"), T + 7);
    set_mtime(&root.join("sub"), T + 9);

    let index = scan(&storage);
    assert_eq!(find(&index, "m.txt").modified_s, (T + 7) as i64);
    assert_eq!(find(&index, "sub").modified_s, (T + 9) as i64);
    let _ = fs::remove_dir_all(&dir);
}

/// A size change is a change even when the modification time did not move.
#[test]
fn test_size_change_is_detected_with_the_same_mtime() {
    let dir = common::fresh_temp_dir("scan_size_change");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("s.txt"), b"abc").unwrap();
    set_mtime(&root.join("s.txt"), T);
    fs::write(root.join("keep.txt"), b"k").unwrap();
    let first = scan(&storage);
    let before = find(&first, "s.txt").clone();

    fs::write(root.join("s.txt"), b"abcdef").unwrap();
    set_mtime(&root.join("s.txt"), T);
    let (second, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    let after = find(&second, "s.txt");
    assert_eq!(after.size, 6);
    assert_eq!(after.blocks.len(), 1);
    assert_eq!(after.blocks[0].size, 6);
    assert!(version_ge(&after.versions, &before.versions));
    assert_ne!(after.versions, before.versions, "a new local version");
    assert!(after.sequence > before.sequence);
    let _ = fs::remove_dir_all(&dir);
}

/// A directory whose time changed is not a new version: directories carry no content.
#[test]
fn test_unchanged_directory_keeps_its_version_when_its_time_changes() {
    let dir = common::fresh_temp_dir("scan_dir_time");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(root.join("d")).unwrap();
    fs::write(root.join("keep.txt"), b"k").unwrap();
    let first = scan(&storage);
    let before = find(&first, "d").clone();

    set_mtime(&root.join("d"), T + 50);
    let (second, state) = storage.build_local_index("f", OWN, &[]).unwrap();
    let after = find(&second, "d");
    assert_eq!(after.versions, before.versions);
    assert_eq!(after.sequence, before.sequence);
    assert_eq!(
        state.max_sequence,
        first.files.iter().map(|f| f.sequence).max().unwrap()
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A file whose only change is its time is re-read, the new time is stored, and nothing is
/// advertised anew.
#[test]
fn test_same_content_with_a_new_mtime_only_updates_the_disk_time() {
    let dir = common::fresh_temp_dir("scan_touch");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("same.txt"), b"abc").unwrap();
    set_mtime(&root.join("same.txt"), T);
    let first = scan(&storage);
    let before = find(&first, "same.txt").clone();
    assert_eq!(stored_db(&storage).entries["same.txt"].disk_mtime, T as i64);

    set_mtime(&root.join("same.txt"), T + 30);
    let (second, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    assert_eq!(find(&second, "same.txt").versions, before.versions);
    assert_eq!(find(&second, "same.txt").sequence, before.sequence);
    let entry = &stored_db(&storage).entries["same.txt"];
    assert_eq!(entry.disk_mtime, (T + 30) as i64, "the new time is saved");
    assert_eq!(entry.sequence, before.sequence);
    let _ = fs::remove_dir_all(&dir);
}

/// A file that reappears on disk while the database still holds a tombstone for it is live
/// again. The database is written by hand (as an earlier run would have left it).
#[test]
fn test_file_recreated_over_a_tombstone_is_live_again() {
    let dir = common::fresh_temp_dir("scan_resurrect");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("f.txt"), b"abcd").unwrap();
    fs::write(root.join("keep.txt"), b"k").unwrap();
    let block_hash = hex(&Sha256::digest(b"abcd"));
    let text = format!(
        "#vita-db-v1\t1\t2\n\
         F\t1\t4\t10\t0\t1\t1:1\t0:4:{block_hash}\tf.txt\n\
         F\t0\t1\t10\t0\t2\t1:1\t\tkeep.txt\n"
    );
    fs::write(storage.db_path("f"), text).unwrap();

    let index = scan(&storage);
    let f = find(&index, "f.txt");
    assert!(!f.deleted, "the file is on disk again");
    assert_eq!(f.size, 4);
    assert!(f
        .versions
        .iter()
        .any(|v| v.short_device_id == 1 && v.value == 1));
    assert!(f
        .versions
        .iter()
        .any(|v| v.short_device_id == short_device_id(OWN)));
    let _ = fs::remove_dir_all(&dir);
}

/// A folder emptied by the user (or by a crash) is not read as deletions when it knows live
/// files, but a removed file is recorded as deleted while others remain.
#[test]
fn test_emptied_folder_keeps_its_live_entries() {
    let dir = common::fresh_temp_dir("scan_guard");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("only.txt"), b"x").unwrap();
    scan(&storage);

    fs::remove_file(root.join("only.txt")).unwrap();
    let index = scan(&storage);
    assert!(
        !find(&index, "only.txt").deleted,
        "an empty root is not a deletion"
    );

    fs::write(root.join("other.txt"), b"y").unwrap();
    fs::write(root.join("gone.txt"), b"z").unwrap();
    scan(&storage);
    fs::remove_file(root.join("gone.txt")).unwrap();
    let index = scan(&storage);
    assert!(find(&index, "gone.txt").deleted);
    assert!(!find(&index, "other.txt").deleted);
    let _ = fs::remove_dir_all(&dir);
}

/// Identical content announced by a valid remote entry adopts that entry's version, so every
/// peer sees one version for the content. Different content or an invalid entry does not.
#[test]
fn test_identical_remote_versions_are_adopted() {
    let dir = common::fresh_temp_dir("scan_adopt");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    for (name, data) in [
        ("m.txt", &b"same"[..]),
        ("n.txt", b"same"),
        ("q.txt", b"mine"),
    ] {
        fs::write(root.join(name), data).unwrap();
    }
    scan(&storage);
    let remote =
        |name: &str, data: &[u8], versions: Vec<BepVersionCounter>, invalid: bool| BepFileInfo {
            name: name.to_owned(),
            is_dir: false,
            size: data.len() as i64,
            modified_s: 1,
            deleted: false,
            invalid,
            sequence: 1,
            versions,
            blocks: vec![BepBlockInfo {
                offset: 0,
                size: data.len() as i32,
                hash: Sha256::digest(data).to_vec(),
                weak_hash: syncthing_core::storage::weak_hash::weak_hash(data),
            }],
            ..Default::default()
        };
    let peer = |v| BepVersionCounter {
        short_device_id: 0x9A7C,
        value: v,
    };
    let remotes = [
        remote("m.txt", b"same", vec![peer(5)], false),
        remote("n.txt", b"same", vec![peer(7)], true),
        remote("q.txt", b"other", vec![peer(9)], false),
    ];
    let (index, _) = storage.build_local_index("f", OWN, &remotes).unwrap();
    assert!(find(&index, "m.txt")
        .versions
        .iter()
        .any(|v| v.short_device_id == 0x9A7C && v.value >= 5));
    assert!(!find(&index, "n.txt")
        .versions
        .iter()
        .any(|v| v.short_device_id == 0x9A7C));
    assert!(!find(&index, "q.txt")
        .versions
        .iter()
        .any(|v| v.short_device_id == 0x9A7C));
    let _ = fs::remove_dir_all(&dir);
}

/// Special files (FIFOs, sockets, devices) are neither indexed nor opened by a scan.
#[cfg(unix)]
#[test]
fn test_named_pipes_are_not_indexed() {
    let dir = common::fresh_temp_dir("scan_fifo");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("ok.txt"), b"ok").unwrap();
    let fifo = root.join("pipe");
    let status = std::process::Command::new("mkfifo").arg(&fifo).status();
    if !matches!(status, Ok(s) if s.success()) {
        let _ = fs::remove_dir_all(&dir);
        return; // mkfifo unavailable: nothing to check.
    }
    // Lets a scan that (wrongly) opens the pipe read end and finish, instead of blocking.
    let writer = fifo.clone();
    std::thread::spawn(move || {
        let _ = fs::OpenOptions::new().write(true).open(writer);
    });

    let index = scan(&storage);
    assert!(index.files.iter().any(|f| f.name == "ok.txt"));
    assert!(!index.files.iter().any(|f| f.name == "pipe"));
    let _ = fs::remove_dir_all(&dir);
}

/// A removed file keeps a tombstone in the database with no size and no blocks.
#[test]
fn test_removed_file_is_stored_as_a_bare_tombstone() {
    let dir = common::fresh_temp_dir("scan_tombstone_db");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("gone.txt"), b"twelve bytes").unwrap();
    fs::write(root.join("keep.txt"), b"k").unwrap();
    scan(&storage);
    assert_eq!(stored_db(&storage).entries["gone.txt"].size, 12);

    fs::remove_file(root.join("gone.txt")).unwrap();
    scan(&storage);
    let entry = &stored_db(&storage).entries["gone.txt"];
    assert!(entry.deleted);
    assert_eq!((entry.size, entry.blocks.len()), (0, 0));
    let _ = fs::remove_dir_all(&dir);
}
