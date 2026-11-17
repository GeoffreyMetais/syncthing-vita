//! What a cycle synchronizes: the app's sharing rules applied to its configuration.

mod common;

use common::{data_dir_with_identity, device, repo, share};
use syncthing_core::{DeviceId, DeviceIdentity, FolderType};
use syncthing_vita_worker::config::WorkerConfig;
use syncthing_vita_worker::defaults::MAX_PEERS_PER_CYCLE;
use syncthing_vita_worker::plan::{SkipReason, SyncPlan};
use vita_syncthing_app::domain::ShareMode;

fn id(seed: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[seed; 32]).unwrap().device_id
}

#[test]
fn test_storage_is_registered_like_the_app_worker() {
    let data = data_dir_with_identity("plan_storage", true);
    let (a, b, c) = (id(1), id(2), id(3));
    let app = repo(&data);
    app.save_devices(&[
        device(a, "A", "1.1.1.1:22000"),
        device(b, "B", "dynamic"),
        device(c, "C", "3.3.3.3:22000"),
    ]);
    let mut ro = share("ro", &data.join("ro"), true, vec![b]);
    ro.mode = ShareMode::ReadOnly;
    app.save_shares(&[
        share("rw", &data.join("rw"), true, vec![a]),
        ro,
        share("paused", &data.join("paused"), false, vec![c]),
        share("lonely", &data.join("lonely"), true, Vec::new()),
        share("danger", std::path::Path::new(&data), true, vec![a]),
    ]);
    let plan = SyncPlan::from_config(&WorkerConfig::load(&data).unwrap());

    let s = &plan.storage;
    assert_eq!(s.root_dir(), data.join("sync"));
    assert_eq!(s.folder_root("rw"), data.join("rw"));
    assert!(s.is_folder_shared_with("rw", &a));
    assert!(!s.is_folder_shared_with("rw", &b));
    assert_eq!(s.folder_type("ro"), FolderType::ReceiveOnly);
    assert_eq!(s.folder_type("rw"), FolderType::SendReceive);
    assert!(s.is_folder_paused("paused"));
    assert!(!s.is_folder_paused("rw"));
    assert!(
        s.is_folder_shared_with("paused", &c),
        "paused shares keep their devices"
    );
    assert!(
        !s.is_folder_shared_with("danger", &a),
        "protected paths are never registered"
    );
    assert!(
        s.local_folders().is_empty(),
        "no folder is offered by the worker"
    );

    let peers: Vec<_> = plan.peers.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(peers, ["A", "B"], "only devices of enabled shares");
    assert_eq!(
        plan.skipped,
        vec![
            ("paused".to_owned(), SkipReason::Paused),
            ("lonely".to_owned(), SkipReason::NoDevice),
            ("danger".to_owned(), SkipReason::ProtectedPath),
        ]
    );
    assert!(!plan.is_empty());
    assert_eq!(plan.peers_left_out, 0);
}

#[test]
fn test_peers_are_bounded_and_unknown_devices_ignored() {
    let data = data_dir_with_identity("plan_bounds", true);
    let ids: Vec<DeviceId> = (10..10 + MAX_PEERS_PER_CYCLE as u8 + 2).map(id).collect();
    let app = repo(&data);
    // The last ID is linked to the share but unknown in devices.cfg: no address, no session.
    let known: Vec<_> = ids[..ids.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, d)| device(*d, &format!("D{i}"), "dynamic"))
        .collect();
    app.save_devices(&known);
    app.save_shares(&[share("all", &data.join("all"), true, ids.clone())]);
    let plan = SyncPlan::from_config(&WorkerConfig::load(&data).unwrap());
    assert_eq!(plan.peers.len(), MAX_PEERS_PER_CYCLE);
    assert_eq!(plan.peers_left_out, 1);
    assert_eq!(plan.peers[0].name, "D0");
    assert_eq!(
        plan.peers[MAX_PEERS_PER_CYCLE - 1].name,
        format!("D{}", MAX_PEERS_PER_CYCLE - 1)
    );
}

#[test]
fn test_without_enabled_share_there_is_nothing_to_do() {
    let data = data_dir_with_identity("plan_empty", true);
    let a = id(20);
    let app = repo(&data);
    app.save_devices(&[device(a, "A", "1.1.1.1:22000")]);
    let plan = SyncPlan::from_config(&WorkerConfig::load(&data).unwrap());
    assert!(
        plan.is_empty(),
        "the app syncs to receive offers, the worker never does"
    );
    app.save_shares(&[share("p", &data.join("p"), false, vec![a])]);
    let plan = SyncPlan::from_config(&WorkerConfig::load(&data).unwrap());
    assert!(plan.is_empty());
}
