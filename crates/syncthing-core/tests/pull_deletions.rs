//! Pull planning for deletions, directories and files that need no transfer: remote deletions
//! (tombstones), directory creation, and which regular files are reported as up to date.

mod common;

use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use syncthing_core::bep::BEP_BLOCK_SIZE;
use syncthing_core::local_db::FolderDb;
use syncthing_core::version_vector::{version_ge, version_merge};
use syncthing_core::{BepBlockInfo, BepFileInfo, BepVersionCounter, DeviceId, FsSyncStorage};

const OWN: DeviceId = DeviceId([0x31; 32]);

fn c(id: u64, value: u64) -> BepVersionCounter {
    BepVersionCounter {
        short_device_id: id,
        value,
    }
}

/// Another device's counter, never the Vita's own.
fn peer(value: u64) -> BepVersionCounter {
    c(0x9A7C, value)
}

fn sha(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

fn blocks_of(data: &[u8]) -> Vec<BepBlockInfo> {
    if data.is_empty() {
        return vec![BepBlockInfo {
            offset: 0,
            size: 0,
            hash: sha(&[]),
            weak_hash: 0,
        }];
    }
    data.chunks(BEP_BLOCK_SIZE)
        .scan(0i64, |offset, chunk| {
            let b = BepBlockInfo {
                offset: *offset,
                size: chunk.len() as i32,
                hash: sha(chunk),
                weak_hash: syncthing_core::storage::weak_hash::weak_hash(chunk),
            };
            *offset += chunk.len() as i64;
            Some(b)
        })
        .collect()
}

fn remote_file(
    name: &str,
    versions: Vec<BepVersionCounter>,
    modified_s: i64,
    data: &[u8],
) -> BepFileInfo {
    BepFileInfo {
        name: name.to_owned(),
        is_dir: false,
        size: data.len() as i64,
        modified_s,
        deleted: false,
        invalid: false,
        sequence: 1,
        versions,
        blocks: blocks_of(data),
        ..Default::default()
    }
}

fn remote_dir(name: &str, versions: Vec<BepVersionCounter>) -> BepFileInfo {
    BepFileInfo {
        is_dir: true,
        ..remote_file(name, versions, 1, &[])
    }
}

fn remote_deleted(name: &str, versions: Vec<BepVersionCounter>) -> BepFileInfo {
    BepFileInfo {
        deleted: true,
        size: 0,
        blocks: Vec::new(),
        ..remote_file(name, versions, 1, &[])
    }
}

fn setup(tag: &str) -> (PathBuf, FsSyncStorage, PathBuf) {
    let dir = common::fresh_temp_dir(tag);
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    (dir, storage, root)
}

/// The Vita's current version vector of `name` (scans the folder first).
fn local_versions(storage: &FsSyncStorage, name: &str) -> Vec<BepVersionCounter> {
    let (index, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    index
        .files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} not in the local index"))
        .versions
        .clone()
}

fn stored_db(storage: &FsSyncStorage) -> FolderDb {
    FolderDb::parse(&fs::read_to_string(storage.db_path("f")).unwrap()).unwrap()
}

#[test]
fn test_remote_deletion_removes_the_file_and_stores_a_tombstone() {
    let (dir, storage, root) = setup("pull_delete");
    fs::write(root.join("d.txt"), b"bye").unwrap();
    let local = local_versions(&storage, "d.txt");
    let merged = version_merge(&local, &[peer(9)]);
    let remote = remote_deleted("d.txt", merged.clone());

    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.applied.iter().any(|m| m == "Deleted d.txt"));
    assert!(!root.join("d.txt").exists());
    let entry = &stored_db(&storage).entries["d.txt"];
    assert!(entry.deleted);
    assert_eq!((entry.size, entry.blocks.len()), (0, 0));
    assert!(
        version_ge(&entry.version, &merged),
        "the merged version is recorded"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_remote_deletion_of_an_unknown_entry_does_nothing() {
    let (dir, storage, root) = setup("pull_delete_unknown");
    fs::write(root.join("other.txt"), b"x").unwrap();
    let remote = remote_deleted("never.txt", vec![peer(2)]);
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.applied.is_empty() && plan.downloads.is_empty());
    assert!(!stored_db(&storage).entries.contains_key("never.txt"));
    let _ = fs::remove_dir_all(&dir);
}

/// A deletion whose file is already gone (its parent is an unreadable entry, so the scan keeps
/// it) is still recorded: the removal failed because nothing is there.
#[cfg(unix)]
#[test]
fn test_deletion_of_an_already_missing_file_is_recorded() {
    let (dir, storage, root) = setup("pull_missing_child");
    fs::create_dir_all(root.join("x")).unwrap();
    fs::write(root.join("x").join("y"), b"data").unwrap();
    fs::write(root.join("keep.txt"), b"k").unwrap();
    let local = local_versions(&storage, "x/y");
    fs::remove_dir_all(root.join("x")).unwrap();
    // A dangling link where the directory was: `x` and everything below it cannot be inspected.
    std::os::unix::fs::symlink(dir.join("missing-target"), root.join("x")).unwrap();

    let remote = remote_deleted("x/y", version_merge(&local, &[peer(9)]));
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.applied.iter().any(|m| m == "Deleted x/y"));
    assert!(stored_db(&storage).entries["x/y"].deleted);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_directory_creation_is_announced_once() {
    let (dir, storage, root) = setup("pull_dirs");
    let plan = storage
        .plan_pull("f", OWN, &[remote_dir("newdir", vec![peer(2)])])
        .unwrap();
    assert!(root.join("newdir").is_dir());
    assert!(plan.applied.iter().any(|m| m == "Created directory newdir"));

    // A directory that is already there is not announced again.
    fs::create_dir_all(root.join("live")).unwrap();
    let local = local_versions(&storage, "live");
    let plan = storage
        .plan_pull(
            "f",
            OWN,
            &[remote_dir("live", version_merge(&local, &[peer(9)]))],
        )
        .unwrap();
    assert!(plan.applied.is_empty(), "{:?}", plan.applied);

    // A directory deleted locally is created again and announced.
    fs::create_dir_all(root.join("old")).unwrap();
    let local = local_versions(&storage, "old");
    fs::remove_dir(root.join("old")).unwrap();
    let (index, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    assert!(index.files.iter().any(|f| f.name == "old" && f.deleted));
    let plan = storage
        .plan_pull(
            "f",
            OWN,
            &[remote_dir("old", version_merge(&local, &[peer(9)]))],
        )
        .unwrap();
    assert!(root.join("old").is_dir());
    assert!(plan.applied.iter().any(|m| m == "Created directory old"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_directory_that_cannot_be_created_is_neither_announced_nor_stored() {
    let (dir, storage, root) = setup("pull_dir_blocked");
    fs::write(root.join("f.txt"), b"a file").unwrap();
    let plan = storage
        .plan_pull("f", OWN, &[remote_dir("f.txt/sub", vec![peer(2)])])
        .unwrap();
    assert!(plan.applied.is_empty(), "{:?}", plan.applied);
    assert!(root.join("f.txt").is_file());
    assert!(!stored_db(&storage).entries.contains_key("f.txt/sub"));
    let _ = fs::remove_dir_all(&dir);
}

/// Only regular files are reported as up to date; directories and tombstones with an equal
/// version are not files to download nor to keep track of.
#[test]
fn test_up_to_date_lists_only_regular_files() {
    let (dir, storage, root) = setup("pull_up_to_date");
    fs::write(root.join("same.txt"), b"abc").unwrap();
    fs::create_dir_all(root.join("d")).unwrap();
    fs::write(root.join("g.txt"), b"gone").unwrap();
    let (index, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    fs::remove_file(root.join("g.txt")).unwrap();
    storage.build_local_index("f", OWN, &[]).unwrap();
    let version_of = |name: &str| {
        index
            .files
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .versions
            .clone()
    };
    let tomb = storage.build_local_index("f", OWN, &[]).unwrap().0;
    let tomb_v = tomb
        .files
        .iter()
        .find(|f| f.name == "g.txt")
        .unwrap()
        .versions
        .clone();

    let remote = vec![
        remote_file("same.txt", version_of("same.txt"), 1, b"abc"),
        remote_dir("d", version_of("d")),
        remote_deleted("g.txt", tomb_v),
    ];
    let plan = storage.plan_pull("f", OWN, &remote).unwrap();
    let names: Vec<&str> = plan.up_to_date.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["same.txt"]);
    assert!(plan.downloads.is_empty());
    let _ = fs::remove_dir_all(&dir);
}
