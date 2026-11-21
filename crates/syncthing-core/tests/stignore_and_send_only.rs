//! `.stignore` pattern filtering and `SendOnly` folder mode + Override Changes tests.

mod common;

use std::fs;
use std::sync::mpsc;
use std::time::Duration;

use common::fake_peer::FakePeer;
use common::fresh_temp_dir;
use syncthing_core::{
    connect_and_sync, BepFileInfo, BepHello, BepIndex, BepMessageType, DeviceId, DeviceIdentity,
    FolderType, FsSyncStorage, IgnoreMatcher, SessionParams, SyncControl, SyncEvent,
    SyncSessionStatus,
};

const PEER_SEED: u8 = 0x55;
const OWN_SEED: u8 = 0x22;

fn own_identity() -> DeviceIdentity {
    DeviceIdentity::from_seed(&[OWN_SEED; 32]).unwrap()
}

fn own_hello() -> BepHello {
    BepHello::new("Vita", "vita-syncthing", "v0.1.0")
}

#[test]
fn stignore_glob_double_star_negation_case_deletable_and_self_exclusion() {
    let dir = fresh_temp_dir("stignore_rules");
    fs::create_dir_all(dir.join("sub/nested")).unwrap();
    fs::create_dir_all(dir.join("inc")).unwrap();
    fs::write(
        dir.join("inc/extra.txt"),
        "// included comment\n(?d)*.bak\n",
    )
    .unwrap();
    fs::write(
        dir.join(".stignore"),
        "\n\
         // Comment line\n\
         !important.tmp\n\
         *.tmp\n\
         **/cache/**\n\
         (?i)readme.md\n\
         /root_only.txt\n\
         ?at.bin\n\
         file[12].dat\n\
         #include inc/extra.txt\n",
    )
    .unwrap();

    let m = IgnoreMatcher::load(&dir);

    // `.stignore` itself is always ignored.
    assert!(m.is_ignored(".stignore"));
    assert!(m.is_ignored("sub/.stignore"));

    // Negation `!important.tmp` precedes `*.tmp`.
    assert!(!m.is_ignored("important.tmp"));
    assert!(m.is_ignored("scratch.tmp"));
    assert!(m.is_ignored("sub/scratch.tmp"));

    // `**` matches across directories.
    assert!(m.is_ignored("a/cache/x.bin"));
    assert!(m.is_ignored("cache/x.bin"));

    // `(?i)` case-insensitive prefix.
    assert!(m.is_ignored("README.MD"));
    assert!(m.is_ignored("sub/ReadMe.md"));

    // Root-anchored `/root_only.txt` only matches at root.
    assert!(m.is_ignored("root_only.txt"));
    assert!(!m.is_ignored("sub/root_only.txt"));

    // `?` and `[abc]` character classes.
    assert!(m.is_ignored("cat.bin"));
    assert!(!m.is_ignored("boat.bin"));
    assert!(m.is_ignored("file1.dat"));
    assert!(m.is_ignored("file2.dat"));
    assert!(!m.is_ignored("file3.dat"));

    // `#include` with `(?d)` deletable tracking.
    assert!(m.is_ignored("save.bak"));
    assert!(m.is_deletable("save.bak"));
    assert!(!m.is_deletable("scratch.tmp"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stignore_include_cycle_and_confinement_guard() {
    let dir = fresh_temp_dir("stignore_cycle");
    let outside = fresh_temp_dir("stignore_outside");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "*.secret\n").unwrap();

    fs::write(dir.join("a.txt"), "#include b.txt\n*.from_a\n").unwrap();
    fs::write(dir.join("b.txt"), "#include a.txt\n*.from_b\n").unwrap();
    fs::write(
        dir.join(".stignore"),
        "#include a.txt\n#include ../stignore_outside/secret.txt\n#include /etc/passwd\n",
    )
    .unwrap();

    let m = IgnoreMatcher::load(&dir);
    assert!(m.is_ignored("foo.from_a"));
    assert!(m.is_ignored("bar.from_b"));
    assert!(!m.is_ignored("x.secret"));

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn stignore_skips_ignored_entries_in_scan_and_pull() {
    let dir = fresh_temp_dir("stignore_scan_pull");
    let root = dir.join("folder");
    fs::create_dir_all(root.join("ignored_dir")).unwrap();
    fs::write(root.join(".stignore"), "*.tmp\nignored_dir/\n").unwrap();
    fs::write(root.join("keep.txt"), b"keep").unwrap();
    fs::write(root.join("skip.tmp"), b"skip").unwrap();
    fs::write(root.join("ignored_dir/inner.txt"), b"inner").unwrap();

    let mut storage = FsSyncStorage::new(&dir);
    storage.set_folder_path("f", &root);
    let own = own_identity();
    let (index, _) = storage
        .build_local_index("f", own.device_id, &[])
        .expect("build_local_index");

    let names: Vec<&str> = index.files.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"keep.txt"));
    assert!(!names.contains(&"skip.tmp"));
    assert!(!names.contains(&".stignore"));
    assert!(!names.iter().any(|n| n.starts_with("ignored_dir")));

    // Remote index announcing ignored files must neither download nor delete them.
    let remote = vec![
        BepFileInfo {
            name: "remote.tmp".to_owned(),
            is_dir: false,
            size: 5,
            modified_s: 100,
            deleted: false,
            invalid: false,
            sequence: 1,
            versions: vec![syncthing_core::BepVersionCounter {
                short_device_id: 99,
                value: 1,
            }],
            blocks: Vec::new(),
        },
        BepFileInfo {
            name: "skip.tmp".to_owned(),
            is_dir: false,
            size: 0,
            modified_s: 100,
            deleted: true,
            invalid: false,
            sequence: 2,
            versions: vec![syncthing_core::BepVersionCounter {
                short_device_id: 99,
                value: 1,
            }],
            blocks: Vec::new(),
        },
        BepFileInfo {
            name: "wanted.txt".to_owned(),
            is_dir: false,
            size: 4,
            modified_s: 100,
            deleted: false,
            invalid: false,
            sequence: 3,
            versions: vec![syncthing_core::BepVersionCounter {
                short_device_id: 99,
                value: 1,
            }],
            blocks: Vec::new(),
        },
    ];
    let plan = storage
        .plan_pull("f", own.device_id, &remote)
        .expect("plan_pull");
    let dl_names: Vec<&str> = plan.downloads.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(dl_names, vec!["wanted.txt"]);
    assert!(root.join("skip.tmp").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn send_only_never_pulls_remote_changes_and_override_bumps_versions() {
    let dir = fresh_temp_dir("send_only_override");
    let folder_root = dir.join("so_folder");
    fs::create_dir_all(&folder_root).unwrap();
    fs::write(folder_root.join("shared.txt"), b"local-authoritative").unwrap();

    let own = own_identity();
    let own_id = own.device_id;
    let control = SyncControl::new();
    let ctrl_trigger = control.clone();

    let peer = FakePeer::spawn(PEER_SEED, move |conn| {
        conn.send_cluster_config(&["so"], own_id);
        let peer_shared = conn.file("so", "shared.txt", b"remote-change", 10);
        let peer_extra = conn.file("so", "remote_only.txt", b"extra-on-peer", 11);
        conn.send_index("so", vec![peer_shared, peer_extra], false);

        // Wait for our initial Index (must NOT request any block from peer).
        let first_idx_payload = conn
            .wait_for(BepMessageType::Index, Duration::from_secs(3))
            .expect("initial Index from SendOnly");
        let first_idx = BepIndex::decode(&first_idx_payload).unwrap();
        let local_shared = first_idx
            .files
            .iter()
            .find(|f| f.name == "shared.txt")
            .expect("shared.txt in initial Index");
        let local_ver_before = local_shared.versions.clone();

        // Trigger Override Changes once the initial sync settled.
        ctrl_trigger.request_override("so");

        // Wait for the IndexUpdate produced by Override Changes.
        let update_payload = conn
            .wait_for(BepMessageType::IndexUpdate, Duration::from_secs(3))
            .expect("IndexUpdate after override");
        let update = BepIndex::decode(&update_payload).unwrap();
        let overridden_shared = update
            .files
            .iter()
            .find(|f| f.name == "shared.txt")
            .expect("shared.txt updated");
        // Bumped version must cover remote version 10 and strictly advance past local_ver_before.
        assert_ne!(overridden_shared.versions, local_ver_before);
        assert!(overridden_shared.versions.iter().any(|c| c.value >= 10));

        let tombstone = update
            .files
            .iter()
            .find(|f| f.name == "remote_only.txt")
            .expect("remote_only.txt tombstone");
        assert!(tombstone.deleted);

        ctrl_trigger.request_stop();
    });

    let mut storage = FsSyncStorage::new(&dir);
    storage.set_folder_path("so", &folder_root);
    storage.set_folder_devices("so", vec![peer.device_id]);
    storage.set_folder_type("so", FolderType::SendOnly);

    let (tx, rx) = mpsc::channel();
    let hello = own_hello();
    let params = SessionParams {
        identity: &own,
        hello: &hello,
        storage: &storage,
        expected_device_id: Some(peer.device_id),
        timeout: Duration::from_secs(5),
        control: Some(control),
    };
    let result = connect_and_sync(&params, peer.addr, move |ev| {
        let _ = tx.send(ev);
    })
    .expect("session");
    let received = peer.join();

    // Never requested remote blocks, and local content is preserved untouched.
    assert!(!received.iter().any(|(t, _)| *t == BepMessageType::Request));
    assert_eq!(
        fs::read(folder_root.join("shared.txt")).unwrap(),
        b"local-authoritative"
    );
    assert!(!folder_root.join("remote_only.txt").exists());
    assert!(matches!(
        result.session_status,
        SyncSessionStatus::Active {
            files_downloaded: 0,
            ..
        }
    ));

    let events: Vec<SyncEvent> = rx.try_iter().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        SyncEvent::RemoteChangesOverridden { folder, overridden } if folder == "so" && *overridden == 2
    )));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn folder_type_wire_values_match_bep_spec() {
    assert_eq!(FolderType::SendReceive.wire_value(), 0);
    assert_eq!(FolderType::SendOnly.wire_value(), 1);
    assert_eq!(FolderType::ReceiveOnly.wire_value(), 2);

    let _dev = DeviceId([0x11; 32]);
}
