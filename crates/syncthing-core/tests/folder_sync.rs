//! Folder reconciliation: version convergence across peers, pull planning and tombstones.

mod common;

use syncthing_core::folder_sync::mask_local_changes;
use syncthing_core::version_vector::{version_ge, version_merge};
use syncthing_core::{BepFileInfo, BepIndex, BepVersionCounter, DeviceId, FsSyncStorage};

fn remote_file(name: &str, versions: Vec<BepVersionCounter>) -> BepFileInfo {
    BepFileInfo {
        name: name.to_owned(),
        is_dir: false,
        size: 0,
        modified_s: 1,
        deleted: false,
        invalid: false,
        sequence: 1,
        versions,
        blocks: Vec::new(),
    }
}

#[test]
fn test_versions_converge_across_peers() {
    let tmp_dir = common::fresh_temp_dir("versions_test");
    let storage = FsSyncStorage::new(&tmp_dir);
    let dev_id = DeviceId([0x22; 32]);
    let root = storage.folder_root("f");
    std::fs::create_dir_all(root.join("PCSE00277")).unwrap();
    std::fs::write(root.join("PCSE00277/save.txt"), b"abc").unwrap();
    std::fs::create_dir_all(root.join("OLD")).unwrap();
    std::fs::write(root.join("gone.txt"), b"x").unwrap();
    let (local, _) = storage.build_local_index("f", dev_id, &[]).unwrap();
    let file = local
        .files
        .iter()
        .find(|f| f.name == "PCSE00277/save.txt")
        .unwrap();
    let gone = local
        .files
        .iter()
        .find(|f| f.name == "gone.txt")
        .unwrap()
        .clone();

    let linux = BepVersionCounter {
        short_device_id: 0x9A7C,
        value: 5,
    };
    let mac = BepVersionCounter {
        short_device_id: 0x4E26,
        value: 7,
    };

    // Peer A (Linux) has the same content with its own version, a directory the Vita lacks,
    // and a newer deletion of `gone.txt`.
    let mut same = file.clone();
    same.versions = vec![linux];
    let mut dir = remote_file("SYNC00001", vec![linux]);
    dir.is_dir = true;
    let mut del = remote_file("gone.txt", version_merge(&gone.versions, &[linux]));
    del.deleted = true;
    let plan = storage
        .plan_pull("f", dev_id, &[same.clone(), dir, del])
        .unwrap();
    assert!(plan.downloads.is_empty());
    assert!(root.join("SYNC00001").is_dir());
    assert!(!root.join("gone.txt").exists());

    // Peer B (Mac) has yet another version of the same content.
    let mut same_b = file.clone();
    same_b.versions = vec![mac];
    storage.plan_pull("f", dev_id, &[same_b.clone()]).unwrap();

    // One single version, newer than or equal to both peers', is advertised to everyone.
    let (idx_a, _) = storage.build_local_index("f", dev_id, &[same]).unwrap();
    let (idx_b, _) = storage.build_local_index("f", dev_id, &[same_b]).unwrap();
    assert_eq!(idx_a.files, idx_b.files);
    let v = &idx_a
        .files
        .iter()
        .find(|f| f.name == "PCSE00277/save.txt")
        .unwrap()
        .versions;
    assert!(version_ge(v, &[linux]) && version_ge(v, &[mac]));
    let t = idx_a.files.iter().find(|f| f.name == "gone.txt").unwrap();
    assert!(t.deleted);
    assert!(idx_a
        .files
        .iter()
        .any(|f| f.name == "SYNC00001" && f.is_dir));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_local_deletion_becomes_tombstone() {
    let tmp_dir = common::fresh_temp_dir("tombstone_test");
    let storage = FsSyncStorage::new(&tmp_dir);
    let dev_id = DeviceId([0x33; 32]);
    let root = storage.folder_root("f");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), b"a").unwrap();
    std::fs::write(root.join("b.txt"), b"b").unwrap();
    let (_, s1) = storage.build_local_index("f", dev_id, &[]).unwrap();
    std::fs::remove_file(root.join("a.txt")).unwrap();
    let (idx, s2) = storage.build_local_index("f", dev_id, &[]).unwrap();
    assert!(s2.max_sequence > s1.max_sequence);
    assert!(idx.files.iter().any(|f| f.name == "a.txt" && f.deleted));

    // An emptied root (e.g. unavailable folder) must not turn everything into deletions.
    std::fs::remove_file(root.join("b.txt")).unwrap();
    let (idx, _) = storage.build_local_index("f", dev_id, &[]).unwrap();
    assert!(idx.files.iter().any(|f| f.name == "b.txt" && !f.deleted));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

/// An entry that exists but cannot be inspected (here a dangling link, on PS Vita a failing
/// `stat()`) must not be advertised as deleted, nor anything below it.
#[cfg(unix)]
#[test]
fn test_unreadable_entry_is_not_recorded_as_deleted() {
    use syncthing_core::{DeviceId, FsSyncStorage};

    let tmp_dir = common::fresh_temp_dir("unreadable_entry");
    let storage = FsSyncStorage::new(&tmp_dir);
    let dev_id = DeviceId([0x11; 32]);
    let dir = storage.folder_root("f");
    std::fs::create_dir_all(dir.join("album")).unwrap();
    std::fs::write(dir.join("save.bin"), b"precious").unwrap();
    std::fs::write(dir.join("album/track.mp3"), b"music").unwrap();
    std::fs::write(dir.join("gone.bin"), b"really deleted").unwrap();
    std::fs::write(dir.join("kept.bin"), b"untouched").unwrap();
    storage.build_local_index("f", dev_id, &[]).unwrap();

    // `save.bin` and `album` become uninspectable; `gone.bin` is really deleted.
    std::fs::remove_file(dir.join("save.bin")).unwrap();
    std::os::unix::fs::symlink(dir.join("missing-target"), dir.join("save.bin")).unwrap();
    std::fs::rename(dir.join("album"), tmp_dir.join("album-aside")).unwrap();
    std::os::unix::fs::symlink(dir.join("missing-target"), dir.join("album")).unwrap();
    std::fs::remove_file(dir.join("gone.bin")).unwrap();

    let (index, _) = storage.build_local_index("f", dev_id, &[]).unwrap();
    let deleted = |name: &str| index.files.iter().find(|f| f.name == name).unwrap().deleted;
    assert!(!deleted("save.bin"));
    assert!(!deleted("album"));
    assert!(!deleted("album/track.mp3"));
    assert!(!deleted("kept.bin"));
    assert!(deleted("gone.bin"));
}

#[test]
fn test_receive_only_masks_local_changes() {
    let peer = |value| BepVersionCounter {
        short_device_id: 0x9A7C,
        value,
    };
    let vita = BepVersionCounter {
        short_device_id: 0x1111,
        value: 1,
    };
    let remote = vec![
        remote_file("same.txt", vec![peer(2)]),
        remote_file("newer_on_peer.txt", vec![peer(5)]),
        remote_file("edited.txt", vec![peer(1)]),
        BepFileInfo {
            invalid: true,
            ..remote_file("peer_invalid.txt", vec![peer(1)])
        },
    ];
    let local = BepIndex {
        folder: "saves".to_owned(),
        files: vec![
            remote_file("same.txt", vec![peer(2)]),
            remote_file("newer_on_peer.txt", vec![peer(3)]),
            remote_file("edited.txt", vec![peer(1), vita]),
            remote_file("created.txt", vec![vita]),
            remote_file("peer_invalid.txt", vec![peer(1)]),
        ],
    };

    let masked = mask_local_changes(&local, &remote);
    let invalid: Vec<(&str, bool)> = masked
        .files
        .iter()
        .map(|f| (f.name.as_str(), f.invalid))
        .collect();
    assert_eq!(
        invalid,
        vec![
            ("same.txt", false),
            ("newer_on_peer.txt", false),
            ("edited.txt", true),
            ("created.txt", true),
            ("peer_invalid.txt", true),
        ]
    );
    // Only the flag changes.
    assert_eq!(masked.files[2].versions, local.files[2].versions);
    assert_eq!(masked.folder, "saves");
}
