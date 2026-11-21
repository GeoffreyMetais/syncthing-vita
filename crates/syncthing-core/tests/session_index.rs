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
fn test_cluster_config_is_sent_again_when_the_shared_folders_change() {
    let vita = device_under_test().device_id;
    let peer = FakePeer::spawn(0x72, move |conn| {
        conn.send_cluster_config(&["alpha"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(5))
            .expect("first ClusterConfig reply");
        // A second configuration with one more folder: our answer must follow it.
        conn.send_cluster_config(&["alpha", "beta"], vita);
        conn.wait_for(BepMessageType::ClusterConfig, Duration::from_secs(3));
        // Stay connected past the exchange window: our initial `Index` follows it.
        conn.serve_until_idle(Duration::from_millis(1500));
    });

    // `alpha` is a local share of the peer from the start; `beta` only becomes one when the
    // peer's second configuration announces it.
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("index_config_resend"));
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
        2,
        "one answer per configuration that changed the set"
    );
    let mut second = announced_folders(configs[1]);
    second.sort();
    assert_eq!(second, vec!["alpha".to_owned(), "beta".to_owned()]);
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
