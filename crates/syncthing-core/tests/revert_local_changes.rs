//! Revert of local changes in a receive-only folder: local-only entries are removed, modified
//! entries lose their local version, and the number of reverted entries is reported.

mod common;

use std::fs;

use syncthing_core::{BepFileInfo, BepIndex, DeviceId, FsSyncStorage};

const OWN: DeviceId = DeviceId([0x66; 32]);

fn as_remote(index: &BepIndex, name: &str) -> BepFileInfo {
    index.files.iter().find(|f| f.name == name).unwrap().clone()
}

#[test]
fn test_revert_removes_local_only_files_and_reports_them() {
    let dir = common::fresh_temp_dir("revert_local_only");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("keep.txt"), b"shared").unwrap();
    fs::write(root.join("extra.txt"), b"mine").unwrap();
    let (local, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    let remote = vec![as_remote(&local, "keep.txt")];

    let reverted = storage
        .revert_local_changes("f", OWN, &remote, &mut || {})
        .unwrap();
    assert_eq!(reverted, 1);
    assert!(!root.join("extra.txt").exists());
    assert_eq!(fs::read(root.join("keep.txt")).unwrap(), b"shared");
    let _ = fs::remove_dir_all(&dir);
}

/// A local-only entry that is already gone from disk (its parent is a dangling link, so the
/// scan cannot inspect it) still counts as reverted: nothing is left to remove.
#[cfg(unix)]
#[test]
fn test_revert_counts_local_entries_already_gone_from_disk() {
    let dir = common::fresh_temp_dir("revert_gone");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(root.join("x")).unwrap();
    fs::write(root.join("x").join("y"), b"data").unwrap();
    fs::write(root.join("keep.txt"), b"k").unwrap();
    let (local, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    let remote = vec![as_remote(&local, "keep.txt")];

    fs::remove_dir_all(root.join("x")).unwrap();
    std::os::unix::fs::symlink(dir.join("missing-target"), root.join("x")).unwrap();
    let reverted = storage
        .revert_local_changes("f", OWN, &remote, &mut || {})
        .unwrap();
    assert_eq!(reverted, 2, "the directory and the file below it");
    let _ = fs::remove_dir_all(&dir);
}

/// A file the peer also has, edited locally, keeps its content on disk; its local version is
/// reset so the peer's version is pulled.
#[test]
fn test_revert_resets_the_version_of_a_modified_shared_file() {
    let dir = common::fresh_temp_dir("revert_modified");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("edit.txt"), b"local edit").unwrap();
    fs::write(root.join("keep.txt"), b"k").unwrap();
    let (local, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    let mut peer_edit = as_remote(&local, "edit.txt");
    peer_edit.versions = vec![syncthing_core::BepVersionCounter {
        short_device_id: 0x1234,
        value: 2,
    }];
    let remote = vec![peer_edit, as_remote(&local, "keep.txt")];

    let reverted = storage
        .revert_local_changes("f", OWN, &remote, &mut || {})
        .unwrap();
    assert_eq!(reverted, 1);
    assert_eq!(fs::read(root.join("edit.txt")).unwrap(), b"local edit");
    let db = syncthing_core::local_db::FolderDb::parse(
        &fs::read_to_string(storage.db_path("f")).unwrap(),
    )
    .unwrap();
    assert!(db.entries["edit.txt"].version.is_empty());
    let _ = fs::remove_dir_all(&dir);
}
