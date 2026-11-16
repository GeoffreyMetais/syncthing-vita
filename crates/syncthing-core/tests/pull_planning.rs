//! Pull planning against a peer's index: which version relation (newer, covered, concurrent)
//! leads to a download, a conflict copy, a deletion, a directory creation or nothing.

mod common;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use syncthing_core::bep::BEP_BLOCK_SIZE;
use syncthing_core::local_db::FolderDb;
use syncthing_core::version_vector::{short_device_id, version_ge, version_merge};
use syncthing_core::{BepBlockInfo, BepFileInfo, BepVersionCounter, DeviceId, FsSyncStorage};

const OWN: DeviceId = DeviceId([0x31; 32]);
const OWN_SHORT: u64 = 0x3131_3131_3131_3131;
const T: u64 = 1_600_000_000;

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
        }];
    }
    data.chunks(BEP_BLOCK_SIZE)
        .scan(0i64, |offset, chunk| {
            let b = BepBlockInfo {
                offset: *offset,
                size: chunk.len() as i32,
                hash: sha(chunk),
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

fn set_mtime(path: &Path, secs: u64) {
    File::open(path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
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

fn conflict_copies(root: &Path, stem: &str) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("{stem}.sync-conflict-"))
        })
        .collect()
}

#[test]
fn test_newer_remote_version_is_downloaded_and_local_file_kept_until_then() {
    let (dir, storage, root) = setup("pull_newer");
    fs::write(root.join("n.txt"), b"old").unwrap();
    let local = local_versions(&storage, "n.txt");
    let remote = remote_file("n.txt", version_merge(&local, &[peer(9)]), 1, b"new");

    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert_eq!(plan.downloads.len(), 1);
    assert_eq!(plan.downloads[0].name, "n.txt");
    assert!(plan.up_to_date.is_empty() && plan.applied.is_empty());
    assert_eq!(fs::read(root.join("n.txt")).unwrap(), b"old");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_remote_version_covered_by_local_is_ignored() {
    let (dir, storage, root) = setup("pull_covered");
    fs::write(root.join("o.txt"), b"data").unwrap();
    local_versions(&storage, "o.txt");
    // The peer only knows an old version of the Vita's own counter: nothing to do.
    let remote = remote_file("o.txt", vec![c(OWN_SHORT, 1)], 1, b"other");
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.downloads.is_empty() && plan.up_to_date.is_empty() && plan.applied.is_empty());
    assert_eq!(fs::read(root.join("o.txt")).unwrap(), b"data");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_own_short_id_matches_the_test_device() {
    assert_eq!(short_device_id(OWN), OWN_SHORT);
}

/// Concurrent versions: a newer remote file wins, and the local file is kept as a conflict copy.
#[test]
fn test_concurrent_newer_remote_wins_and_local_copy_is_kept() {
    let (dir, storage, root) = setup("pull_conflict_remote");
    fs::write(root.join("c.txt"), b"local").unwrap();
    set_mtime(&root.join("c.txt"), T);
    let local = local_versions(&storage, "c.txt");
    let remote = remote_file("c.txt", vec![peer(3)], (T + 100) as i64, b"remote");
    assert!(
        !version_ge(&remote.versions, &local),
        "versions must be concurrent"
    );

    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan
        .applied
        .iter()
        .any(|m| m.starts_with("Conflict on c.txt")));
    assert_eq!(plan.downloads.len(), 1);
    assert!(!root.join("c.txt").exists());
    let copies = conflict_copies(&root, "c");
    assert_eq!(copies.len(), 1);
    assert_eq!(fs::read(&copies[0]).unwrap(), b"local");
    let _ = fs::remove_dir_all(&dir);
}

/// Concurrent versions with an older or equally old remote file: the local file wins, nothing
/// is downloaded and nothing is renamed.
#[test]
fn test_concurrent_older_or_equal_remote_loses() {
    let (dir, storage, root) = setup("pull_conflict_local");
    for (name, remote_time) in [("older.txt", T - 100), ("equal.txt", T)] {
        fs::write(root.join(name), b"local").unwrap();
        set_mtime(&root.join(name), T);
        let local = local_versions(&storage, name);
        let remote = remote_file(name, vec![peer(3)], remote_time as i64, b"remote");
        assert!(!version_ge(&remote.versions, &local));
        let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
        assert!(plan.downloads.is_empty(), "{name}: local version must win");
        assert!(plan.applied.is_empty(), "{name}: no conflict copy");
        assert_eq!(fs::read(root.join(name)).unwrap(), b"local");
    }
    assert!(conflict_copies(&root, "older").is_empty());
    assert!(conflict_copies(&root, "equal").is_empty());
    let _ = fs::remove_dir_all(&dir);
}

/// A deletion always loses against a concurrent live file, whatever the timestamps.
#[test]
fn test_concurrent_live_remote_beats_a_local_tombstone_even_if_older() {
    let (dir, storage, root) = setup("pull_tomb_vs_live");
    // A second file keeps the folder non-empty (an empty root is never read as deletions).
    fs::write(root.join("other.txt"), b"x").unwrap();
    fs::write(root.join("t.txt"), b"bye").unwrap();
    set_mtime(&root.join("t.txt"), T);
    local_versions(&storage, "t.txt");
    fs::remove_file(root.join("t.txt")).unwrap();
    let (index, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    let tomb = index.files.iter().find(|f| f.name == "t.txt").unwrap();
    assert!(tomb.deleted);

    let remote = remote_file("t.txt", vec![peer(4)], 1, b"back");
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert_eq!(plan.downloads.len(), 1, "the live remote file is pulled");
    assert_eq!(plan.downloads[0].name, "t.txt");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_concurrent_remote_deletion_loses_against_a_local_file() {
    let (dir, storage, root) = setup("pull_live_vs_tomb");
    fs::write(root.join("k.txt"), b"keep").unwrap();
    local_versions(&storage, "k.txt");
    let remote = remote_deleted("k.txt", vec![peer(4)]);
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.applied.is_empty() && plan.downloads.is_empty());
    assert_eq!(fs::read(root.join("k.txt")).unwrap(), b"keep");
    let _ = fs::remove_dir_all(&dir);
}

/// A newer remote file over a local directory of the same name: the directory is not renamed
/// as a conflict copy, the remote file is pulled.
#[test]
fn test_concurrent_file_over_local_directory_keeps_the_directory() {
    let (dir, storage, root) = setup("pull_dir_conflict");
    fs::create_dir_all(root.join("x")).unwrap();
    set_mtime(&root.join("x"), T);
    local_versions(&storage, "x");
    let remote = remote_file("x", vec![peer(4)], (T + 100) as i64, b"file");

    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(
        root.join("x").is_dir(),
        "the local directory stays in place"
    );
    assert!(!plan.applied.iter().any(|m| m.starts_with("Conflict")));
    assert_eq!(plan.downloads.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

/// A concurrent version with the same content is merged, never downloaded.
#[test]
fn test_same_content_with_another_version_is_merged_and_up_to_date() {
    let (dir, storage, root) = setup("pull_same_content");
    fs::write(root.join("m.txt"), b"abc").unwrap();
    let local = local_versions(&storage, "m.txt");
    let remote = remote_file("m.txt", vec![peer(3)], 1, b"abc");
    assert!(!version_ge(&remote.versions, &local));
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.downloads.is_empty());
    assert_eq!(plan.up_to_date.len(), 1);
    let merged = &stored_db(&storage).entries["m.txt"].version;
    assert!(version_ge(merged, &[peer(3)]) && version_ge(merged, &local));
    let _ = fs::remove_dir_all(&dir);
}

/// A local file with the right blocks but trailing data is downloaded again (fixed in the temp
/// file); a file of the right length with other content is downloaded too.
#[test]
fn test_local_file_is_only_up_to_date_when_blocks_and_length_match() {
    let (dir, storage, root) = setup("pull_length");
    fs::write(root.join("t.bin"), b"abcdef").unwrap();
    fs::write(root.join("s.bin"), b"abc").unwrap();
    let tv = local_versions(&storage, "t.bin");
    let sv = local_versions(&storage, "s.bin");
    let remote = vec![
        remote_file("t.bin", version_merge(&tv, &[peer(3)]), 1, b"abc"),
        remote_file("s.bin", version_merge(&sv, &[peer(3)]), 1, b"xyz"),
    ];
    let plan = storage.plan_pull("f", OWN, &remote).unwrap();
    let mut downloads: Vec<&str> = plan.downloads.iter().map(|f| f.name.as_str()).collect();
    downloads.sort_unstable();
    assert_eq!(downloads, vec!["s.bin", "t.bin"]);
    assert!(plan.up_to_date.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

/// A concurrent remote deletion that is newer than the local live file still loses: a deletion
/// only wins against a deletion.
#[test]
fn test_newer_concurrent_remote_deletion_still_loses_against_a_local_file() {
    let (dir, storage, root) = setup("pull_live_vs_newer_tomb");
    fs::write(root.join("k.txt"), b"keep").unwrap();
    set_mtime(&root.join("k.txt"), T);
    local_versions(&storage, "k.txt");
    let remote = BepFileInfo {
        modified_s: (T + 100) as i64,
        ..remote_deleted("k.txt", vec![peer(4)])
    };
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.applied.is_empty() && plan.downloads.is_empty());
    assert_eq!(fs::read(root.join("k.txt")).unwrap(), b"keep");
    let _ = fs::remove_dir_all(&dir);
}

/// The same bytes split into other blocks by the peer: the block descriptions differ, but the
/// local data matches them, so the file is up to date without a download.
#[test]
fn test_same_bytes_with_another_block_layout_are_up_to_date() {
    let (dir, storage, root) = setup("pull_other_layout");
    fs::write(root.join("l.txt"), b"abc").unwrap();
    let local = local_versions(&storage, "l.txt");
    let mut remote = remote_file("l.txt", version_merge(&local, &[peer(3)]), 1, b"abc");
    remote.blocks = vec![
        BepBlockInfo {
            offset: 0,
            size: 2,
            hash: sha(b"ab"),
        },
        BepBlockInfo {
            offset: 2,
            size: 1,
            hash: sha(b"c"),
        },
    ];
    let plan = storage.plan_pull("f", OWN, &[remote]).unwrap();
    assert!(plan.downloads.is_empty());
    assert_eq!(plan.up_to_date.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}
