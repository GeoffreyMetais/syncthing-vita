//! Folder announcements and index publication of a session: which folders the initial
//! `ClusterConfig` lists, when it is sent again, and how runtime `ClusterConfig`s are applied.

mod common;

use std::time::Duration;

use common::fake_peer::FakePeer;
use common::session_driver::{
    announced_folders, control_with, device_under_test, hello_under_test, index_messages, payloads,
    run_session,
};
use syncthing_core::{BepFolder, BepMessageType, FsSyncStorage, SyncEvent};

/// A storage with `folders` (dirs created, each with one local file) linked to `peer` only
/// for those in `linked`.
fn storage_with(
    name: &str,
    peer: syncthing_core::DeviceId,
    folders: &[&str],
    linked: &[&str],
) -> FsSyncStorage {
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir(name));
    for folder in folders {
        if linked.contains(folder) {
            storage.set_folder_devices(folder, vec![peer]);
            storage.add_local_folder(BepFolder {
                id: (*folder).to_owned(),
                label: (*folder).to_owned(),
                device_ids: vec![peer],
                ..Default::default()
            });
        }
        let dir = storage.folder_root(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("local.txt"), folder.as_bytes()).unwrap();
    }
    storage
}

#[test]
fn test_cluster_config_lists_linked_folders_and_locally_shared_ones() {
    let vita = device_under_test().device_id;
    // The peer announces `shared` only. `private` is shared locally with the peer but the peer
    // does not announce it: our `ClusterConfig` still lists it, and we never publish it.
    let peer = FakePeer::spawn(0x71, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    let storage = storage_with(
        "index_announce",
        peer.device_id,
        &["shared", "private"],
        &["shared", "private"],
    );
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(control_with(
            Duration::from_secs(3600),
            Duration::from_secs(20),
        )),
        |_| {},
    )
    .expect("session failed");
    let received = peer.join();

    let configs = payloads(&received, BepMessageType::ClusterConfig);
    let mut announced = announced_folders(configs[0]);
    announced.sort();
    assert_eq!(announced, vec!["private".to_owned(), "shared".to_owned()]);

    let published: Vec<String> = index_messages(&received)
        .into_iter()
        .map(|i| i.folder)
        .collect();
    assert!(published.contains(&"shared".to_owned()));
    assert!(!published.contains(&"private".to_owned()));
}

#[test]
fn test_cluster_config_is_sent_at_most_once_per_connection() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x72, move |conn| {
        conn.send_cluster_config(&["alpha"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("first ClusterConfig reply");
        // A second ClusterConfig with an additional folder must NOT trigger a second
        // ClusterConfig from us (fatal protocol error in Syncthing state 1), yet we still
        // accept `beta` and publish its initial `Index`.
        conn.send_cluster_config(&["alpha", "beta"], vita);
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    // `alpha` is a local share of the peer from the start; `beta` only becomes one when the
    // peer's second configuration announces it.
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("index_config_once"));
    storage.set_folder_devices("alpha", vec![peer.device_id]);
    storage.set_folder_devices("beta", vec![peer.device_id]);
    storage.add_local_folder(BepFolder {
        id: "alpha".to_owned(),
        label: "alpha".to_owned(),
        device_ids: vec![peer.device_id],
        ..Default::default()
    });
    for folder in ["alpha", "beta"] {
        let dir = storage.folder_root(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("local.txt"), folder.as_bytes()).unwrap();
    }
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(control_with(
            Duration::from_secs(3600),
            Duration::from_secs(20),
        )),
        |_| {},
    )
    .expect("session failed");
    let received = peer.join();

    let configs = payloads(&received, BepMessageType::ClusterConfig);
    assert_eq!(
        configs.len(),
        1,
        "BEP/1.0 forbids sending a second ClusterConfig on a connection"
    );
    assert_eq!(announced_folders(configs[0]), vec!["alpha".to_owned()]);

    let published: Vec<String> = index_messages(&received)
        .into_iter()
        .map(|i| i.folder)
        .collect();
    assert!(published.contains(&"alpha".to_owned()));
    assert!(published.contains(&"beta".to_owned()));
}

#[test]
fn test_initial_cluster_config_uses_db_state_without_pre_config_ping() {
    let vita_identity = device_under_test();
    let vita = vita_identity.device_id;
    let peer = FakePeer::spawn(0x74, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of shared folder");
        conn.serve_until_idle(Duration::from_millis(600));
    });

    let storage = storage_with(
        "index_no_pre_ping",
        peer.device_id,
        &["shared"],
        &["shared"],
    );
    // Pre-populate the DB so `folder_index_state` has a known non-zero `index_id` & `max_sequence`.
    let (_, expected_state) = storage
        .build_local_index("shared", vita, &[])
        .expect("initial local scan");
    assert_ne!(expected_state.index_id, 0);
    assert!(expected_state.max_sequence > 0);
    let db_state = storage.folder_index_state("shared", vita);
    assert_eq!(db_state.index_id, expected_state.index_id);
    assert_eq!(db_state.max_sequence, expected_state.max_sequence);
    assert!(!db_state.paused);

    // Zero keepalive interval: if `exchange_cluster_config` ran `scan_local_index`, it would send
    // `Ping` before `ClusterConfig` (fatal `ping message in state 0` in Syncthing).
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(control_with(Duration::from_secs(3600), Duration::ZERO)),
        |_| {},
    )
    .expect("session failed");
    let received = peer.join();

    assert_eq!(
        received.first().map(|(t, _)| *t),
        Some(BepMessageType::ClusterConfig),
        "first message after Hello must be ClusterConfig, never Ping"
    );
    let cfg = syncthing_core::BepClusterConfig::decode(
        payloads(&received, BepMessageType::ClusterConfig)[0],
    )
    .unwrap();
    assert_eq!(cfg.folders[0].id, "shared");
}

#[test]
fn test_runtime_cluster_config_only_adds_linked_and_running_folders() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x73, move |conn| {
        conn.send_cluster_config(&["shared"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("ClusterConfig reply");
        conn.send_index("shared", Vec::new(), false);
        conn.wait_for(BepMessageType::Index, Duration::from_secs(5))
            .expect("Index of the shared folder");
        // Later the peer announces two more folders: `secret` is not shared with it, `paused`
        // is shared but paused on our side.
        conn.send_cluster_config(&["shared", "secret", "paused"], vita);
        conn.serve_until_idle(Duration::from_millis(800));
    });

    let mut storage = storage_with(
        "index_runtime_config",
        peer.device_id,
        &["shared", "secret", "paused"],
        &["shared", "paused"],
    );
    storage.set_folder_paused("paused", true);
    let mut events = Vec::new();
    run_session(
        &peer,
        &storage,
        &hello_under_test(1),
        Some(control_with(
            Duration::from_secs(3600),
            Duration::from_secs(20),
        )),
        |e| events.push(e),
    )
    .expect("session failed");
    let received = peer.join();

    let published: Vec<String> = index_messages(&received)
        .into_iter()
        .map(|i| i.folder)
        .collect();
    assert!(published.contains(&"shared".to_owned()));
    assert!(
        !published.contains(&"secret".to_owned()),
        "unshared folder published"
    );
    assert!(
        !published.contains(&"paused".to_owned()),
        "paused folder published"
    );

    // The application still learns about every announced folder, to ask for consent.
    let offered = events.iter().any(|e| match e {
        SyncEvent::ClusterConfigReceived { folders, .. } => folders.len() == 3,
        _ => false,
    });
    assert!(offered, "runtime ClusterConfig not reported");
}
