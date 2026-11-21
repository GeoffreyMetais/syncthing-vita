//! Integration tests for Milestone P2.4:
//! - Extended `BepFileInfo` metadata (`permissions`, `no_permissions`, `modified_ns`,
//!   `modified_by`, `symlink_target` with `SYMLINK = 4`).
//! - Extended `BepClusterConfig` options (`introducer`, `max_request_kib`, `ignore_permissions`,
//!   `ignore_delete`, and `BepClusterDevice`).
//! - Live localhost sync with `sync_symlinks = true` (Unix) and `sync_symlinks = false`.
//! - `SyncEvent::DevicesIntroduced` emitted when an introducer peer announces unknown devices.

mod common;

use std::fs;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use syncthing_core::local_db::FolderDb;
use syncthing_core::{
    accept_and_sync, connect_and_sync, BepBlockInfo, BepClusterConfig, BepClusterDevice,
    BepDeviceCompression, BepFileInfo, BepFolder, BepHello, BepIndex, BepVersionCounter, DeviceId,
    DeviceIdentity, FsSyncStorage, SessionParams, SyncEvent,
};

#[test]
fn bep_file_info_extended_metadata_and_symlink_roundtrip() {
    let file = BepFileInfo {
        name: "bin/run.sh".to_owned(),
        is_dir: false,
        size: 12,
        permissions: 0o755,
        modified_s: 1_700_000_123,
        modified_ns: 456_789_012,
        modified_by: 0x0A0B_0C0D_0E0F_1011,
        deleted: false,
        invalid: false,
        no_permissions: false,
        sequence: 9,
        versions: vec![BepVersionCounter {
            short_device_id: 0x0A0B_0C0D_0E0F_1011,
            value: 4,
        }],
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: 12,
            hash: vec![0xAB; 32],
            weak_hash: 0x1234_5678,
        }],
        symlink_target: None,
    };

    let symlink = BepFileInfo {
        name: "current".to_owned(),
        is_dir: false,
        size: 0,
        permissions: 0o777,
        modified_s: 1_700_000_200,
        modified_ns: 987_654_321,
        modified_by: 0x1122_3344_5566_7788,
        deleted: false,
        invalid: false,
        no_permissions: true,
        sequence: 10,
        versions: vec![BepVersionCounter {
            short_device_id: 0x1122_3344_5566_7788,
            value: 1,
        }],
        blocks: Vec::new(),
        symlink_target: Some("releases/v1.0".to_owned()),
    };

    let idx = BepIndex {
        folder: "shared".to_owned(),
        files: vec![file.clone(), symlink.clone()],
    };
    let decoded = BepIndex::decode(&idx.encode()).expect("decode BepIndex");
    assert_eq!(decoded.folder, "shared");
    assert_eq!(decoded.files, vec![file, symlink]);
    assert!(decoded.files[1].is_symlink());
    assert!(!decoded.files[1].invalid);
}

#[test]
fn local_db_persists_extended_metadata_and_symlinks() {
    let dir = common::fresh_temp_dir("p24_local_db_meta");
    let mut storage = FsSyncStorage::new(&dir);
    storage.set_sync_symlinks(cfg!(unix));
    storage.set_ignore_permissions(!cfg!(unix));

    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("script.sh"), b"#!/bin/sh\necho ok\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("script.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("script.sh", root.join("link_to_script")).unwrap();
    }

    let own = DeviceId([0x21; 32]);
    let (idx, _) = storage.build_local_index("f", own, &[]).unwrap();
    let script = idx
        .files
        .iter()
        .find(|f| f.name == "script.sh")
        .expect("script.sh");
    #[cfg(unix)]
    {
        assert_eq!(script.permissions, 0o755);
        assert!(!script.no_permissions);
        let link = idx
            .files
            .iter()
            .find(|f| f.name == "link_to_script")
            .expect("symlink");
        assert_eq!(link.symlink_target.as_deref(), Some("script.sh"));
        assert!(link.blocks.is_empty());
    }

    let serialized = fs::read_to_string(storage.db_path("f")).unwrap();
    let parsed = FolderDb::parse(&serialized).expect("re-parse db");
    assert_eq!(
        parsed.entries.get("script.sh").map(|e| e.permissions),
        Some(script.permissions)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bep_cluster_config_extended_fields_roundtrip() {
    let dev_a = DeviceId([0xA1; 32]);
    let dev_b = DeviceId([0xB2; 32]);
    let cfg = BepClusterConfig {
        folders: vec![BepFolder {
            id: "code".to_owned(),
            label: "Code".to_owned(),
            ignore_permissions: true,
            ignore_delete: true,
            device_ids: vec![dev_a, dev_b],
            devices: vec![
                BepClusterDevice {
                    id: dev_a,
                    name: "Alice".to_owned(),
                    compression: BepDeviceCompression::Always,
                    introducer: true,
                    max_request_kib: 4096,
                },
                BepClusterDevice {
                    id: dev_b,
                    name: "Bob".to_owned(),
                    compression: BepDeviceCompression::Never,
                    introducer: false,
                    max_request_kib: 512,
                },
            ],
        }],
        secondary: false,
    };

    let encoded = cfg.encode();
    let decoded = BepClusterConfig::decode(&encoded).expect("decode ClusterConfig");
    assert_eq!(decoded, cfg);
}

#[cfg(unix)]
#[test]
fn live_sync_transfers_symlinks_when_enabled_and_skips_when_disabled() {
    let id_a = DeviceIdentity::from_seed(&[0x41; 32]).unwrap();
    let id_b = DeviceIdentity::from_seed(&[0x42; 32]).unwrap();
    let id_c = DeviceIdentity::from_seed(&[0x43; 32]).unwrap();

    // Case 1: peer B has sync_symlinks = true (Unix), peer C has sync_symlinks = false (Vita-like).
    let dir_a = common::fresh_temp_dir("p24_sym_a");
    let dir_b = common::fresh_temp_dir("p24_sym_b");
    let dir_c = common::fresh_temp_dir("p24_sym_c");

    let mut storage_a = FsSyncStorage::new(&dir_a);
    storage_a.set_sync_symlinks(true);
    storage_a.set_ignore_permissions(false);
    let root_a = storage_a.folder_root("shared");
    fs::create_dir_all(&root_a).unwrap();
    fs::write(root_a.join("hello.txt"), b"hello symlink").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root_a.join("hello.txt"), fs::Permissions::from_mode(0o750)).unwrap();
    }
    std::os::unix::fs::symlink("hello.txt", root_a.join("alias.txt")).unwrap();
    storage_a.set_folder_devices("shared", vec![id_b.device_id, id_c.device_id]);
    storage_a.add_local_folder(BepFolder {
        id: "shared".to_owned(),
        label: "Shared".to_owned(),
        device_ids: vec![id_b.device_id, id_c.device_id],
        ..Default::default()
    });

    // Peer B accepts symlinks and permissions.
    let mut storage_b = FsSyncStorage::new(&dir_b);
    storage_b.set_sync_symlinks(true);
    storage_b.set_ignore_permissions(false);
    fs::create_dir_all(storage_b.folder_root("shared")).unwrap();
    storage_b.set_folder_devices("shared", vec![id_a.device_id]);
    storage_b.add_local_folder(BepFolder {
        id: "shared".to_owned(),
        label: "Shared".to_owned(),
        device_ids: vec![id_a.device_id],
        ..Default::default()
    });

    // Sync A -> B
    let listener_b = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr_b = listener_b.local_addr().unwrap();
    let hello_a = BepHello::new("PeerA", "syncthing-core-test", "0.1.0");
    let hello_b = BepHello::new("PeerB", "syncthing-core-test", "0.1.0");

    let id_b_srv = id_b.clone();
    let srv_b = thread::spawn(move || {
        let (tcp, _) = listener_b.accept().unwrap();
        let incoming =
            syncthing_core::accept_bep_tls_stream(&id_b_srv, tcp, Duration::from_secs(5)).unwrap();
        let params = SessionParams {
            identity: &id_b_srv,
            hello: &hello_b,
            storage: &storage_b,
            expected_device_id: Some(id_a.device_id),
            timeout: Duration::from_secs(5),
            control: None,
            secondary_inbox: None,
        };
        accept_and_sync(&params, incoming, addr_b, |_| {}).unwrap();
    });

    let params_a = SessionParams {
        identity: &id_a,
        hello: &hello_a,
        storage: &storage_a,
        expected_device_id: Some(id_b.device_id),
        timeout: Duration::from_secs(5),
        control: None,
        secondary_inbox: None,
    };
    connect_and_sync(&params_a, addr_b, |_| {}).unwrap();
    srv_b.join().unwrap();

    let root_b = FsSyncStorage::new(&dir_b).folder_root("shared");
    assert_eq!(
        fs::read(root_b.join("hello.txt")).unwrap(),
        b"hello symlink"
    );
    let link_target = fs::read_link(root_b.join("alias.txt")).expect("symlink pulled to B");
    assert_eq!(link_target.to_string_lossy(), "hello.txt");
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(root_b.join("hello.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o750);
    }

    // Peer C has sync_symlinks = false (PS Vita / FAT behavior): skips symlink cleanly.
    let mut storage_c = FsSyncStorage::new(&dir_c);
    storage_c.set_sync_symlinks(false);
    fs::create_dir_all(storage_c.folder_root("shared")).unwrap();
    storage_c.set_folder_devices("shared", vec![id_a.device_id]);
    storage_c.add_local_folder(BepFolder {
        id: "shared".to_owned(),
        label: "Shared".to_owned(),
        device_ids: vec![id_a.device_id],
        ..Default::default()
    });

    let listener_c = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr_c = listener_c.local_addr().unwrap();
    let hello_c = BepHello::new("PeerC", "syncthing-core-test", "0.1.0");
    let id_c_srv = id_c.clone();
    let srv_c = thread::spawn(move || {
        let (tcp, _) = listener_c.accept().unwrap();
        let incoming =
            syncthing_core::accept_bep_tls_stream(&id_c_srv, tcp, Duration::from_secs(5)).unwrap();
        let params = SessionParams {
            identity: &id_c_srv,
            hello: &hello_c,
            storage: &storage_c,
            expected_device_id: Some(id_a.device_id),
            timeout: Duration::from_secs(5),
            control: None,
            secondary_inbox: None,
        };
        accept_and_sync(&params, incoming, addr_c, |_| {}).unwrap();
    });

    let params_ac = SessionParams {
        identity: &id_a,
        hello: &hello_a,
        storage: &storage_a,
        expected_device_id: Some(id_c.device_id),
        timeout: Duration::from_secs(5),
        control: None,
        secondary_inbox: None,
    };
    connect_and_sync(&params_ac, addr_c, |_| {}).unwrap();
    srv_c.join().unwrap();

    let root_c = FsSyncStorage::new(&dir_c).folder_root("shared");
    assert_eq!(
        fs::read(root_c.join("hello.txt")).unwrap(),
        b"hello symlink"
    );
    assert!(
        fs::symlink_metadata(root_c.join("alias.txt")).is_err(),
        "symlink must not be created when sync_symlinks is false"
    );

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
    let _ = fs::remove_dir_all(&dir_c);
}

#[test]
fn introducer_peer_emits_devices_introduced_event() {
    let id_intro = DeviceIdentity::from_seed(&[0x51; 32]).unwrap();
    let id_local = DeviceIdentity::from_seed(&[0x52; 32]).unwrap();
    let id_third = DeviceId([0x53; 32]);

    let dir_intro = common::fresh_temp_dir("p24_intro_a");
    let dir_local = common::fresh_temp_dir("p24_intro_b");

    let mut storage_intro = FsSyncStorage::new(&dir_intro);
    fs::create_dir_all(storage_intro.folder_root("team")).unwrap();
    storage_intro.set_folder_devices("team", vec![id_local.device_id, id_third]);
    storage_intro.add_local_folder(BepFolder {
        id: "team".to_owned(),
        label: "Team".to_owned(),
        device_ids: vec![id_local.device_id, id_third],
        devices: vec![
            BepClusterDevice {
                id: id_local.device_id,
                name: "Local".to_owned(),
                ..Default::default()
            },
            BepClusterDevice {
                id: id_third,
                name: "Charlie".to_owned(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });

    let mut storage_local = FsSyncStorage::new(&dir_local);
    fs::create_dir_all(storage_local.folder_root("team")).unwrap();
    storage_local.set_folder_devices("team", vec![id_intro.device_id]);
    storage_local.set_device_introducer(id_intro.device_id, true);
    storage_local.add_local_folder(BepFolder {
        id: "team".to_owned(),
        label: "Team".to_owned(),
        device_ids: vec![id_intro.device_id],
        ..Default::default()
    });

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let hello_intro = BepHello::new("Introducer", "syncthing-core-test", "0.1.0");
    let hello_local = BepHello::new("Local", "syncthing-core-test", "0.1.0");

    let id_intro_srv = id_intro.clone();
    let srv = thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let incoming =
            syncthing_core::accept_bep_tls_stream(&id_intro_srv, tcp, Duration::from_secs(5))
                .unwrap();
        let params = SessionParams {
            identity: &id_intro_srv,
            hello: &hello_intro,
            storage: &storage_intro,
            expected_device_id: Some(id_local.device_id),
            timeout: Duration::from_secs(5),
            control: None,
            secondary_inbox: None,
        };
        accept_and_sync(&params, incoming, addr, |_| {}).unwrap();
    });

    let mut events = Vec::new();
    let params_local = SessionParams {
        identity: &id_local,
        hello: &hello_local,
        storage: &storage_local,
        expected_device_id: Some(id_intro.device_id),
        timeout: Duration::from_secs(5),
        control: None,
        secondary_inbox: None,
    };
    connect_and_sync(&params_local, addr, |ev| events.push(ev)).unwrap();
    srv.join().unwrap();

    let introduced: Vec<(DeviceId, Vec<(DeviceId, String)>)> = events
        .into_iter()
        .filter_map(|ev| match ev {
            SyncEvent::DevicesIntroduced {
                introducer,
                devices,
            } => Some((introducer, devices)),
            _ => None,
        })
        .collect();

    assert_eq!(
        introduced.len(),
        1,
        "expected exactly one DevicesIntroduced"
    );
    assert_eq!(introduced[0].0, id_intro.device_id);
    assert_eq!(introduced[0].1, vec![(id_third, "Charlie".to_owned())]);

    let _ = fs::remove_dir_all(&dir_intro);
    let _ = fs::remove_dir_all(&dir_local);
}
