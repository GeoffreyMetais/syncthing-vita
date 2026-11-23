//! Folder registry of `FsSyncStorage`: advertised local folders, pause state, and the devices
//! each folder is shared with (the authorization gate of every session).

use std::path::PathBuf;

use syncthing_core::{BepFolder, DeviceId, FsSyncStorage};

fn folder(id: &str, label: &str, devices: Vec<DeviceId>) -> BepFolder {
    BepFolder {
        id: id.to_owned(),
        label: label.to_owned(),
        device_ids: devices,
        ..Default::default()
    }
}

fn storage() -> FsSyncStorage {
    FsSyncStorage::new(PathBuf::from("registry-test-root"))
}

#[test]
fn test_local_folders_are_listed_and_replaced_by_id() {
    let mut s = storage();
    assert!(s.local_folders().is_empty());

    s.add_local_folder(folder("a", "Alpha", Vec::new()));
    s.add_local_folder(folder("b", "Beta", Vec::new()));
    let ids: Vec<&str> = s.local_folders().iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);

    // Re-adding a folder id replaces that folder only, in place.
    s.add_local_folder(folder("b", "Beta renamed", Vec::new()));
    let labels: Vec<(&str, &str)> = s
        .local_folders()
        .iter()
        .map(|f| (f.id.as_str(), f.label.as_str()))
        .collect();
    assert_eq!(labels, vec![("a", "Alpha"), ("b", "Beta renamed")]);
}

#[test]
fn test_paused_state_is_per_folder() {
    let mut s = storage();
    assert!(!s.is_folder_paused("saves"));
    s.set_folder_paused("saves", true);
    assert!(s.is_folder_paused("saves"));
    assert!(!s.is_folder_paused("music"));
    s.set_folder_paused("saves", false);
    assert!(!s.is_folder_paused("saves"));
}

#[test]
fn test_folder_is_shared_only_with_its_linked_devices() {
    let mut s = storage();
    let linked = DeviceId([0xA1; 32]);
    let other = DeviceId([0xB2; 32]);
    assert!(
        !s.is_folder_shared_with("saves", &linked),
        "unconfigured folder"
    );

    s.set_folder_devices("saves", vec![linked]);
    assert!(s.is_folder_shared_with("saves", &linked));
    assert!(!s.is_folder_shared_with("saves", &other), "unlinked peer");
    assert!(!s.is_folder_shared_with("music", &linked), "another folder");

    // Replacing the list replaces the authorization.
    s.set_folder_devices("saves", vec![other]);
    assert!(!s.is_folder_shared_with("saves", &linked));
    assert!(s.is_folder_shared_with("saves", &other));
    s.set_folder_devices("saves", Vec::new());
    assert!(!s.is_folder_shared_with("saves", &other));
}

#[test]
fn test_configured_folders_for_peer_uses_labels_and_filters_by_peer() {
    let mut s = storage();
    let peer_a = DeviceId([0xA1; 32]);
    let peer_b = DeviceId([0xB2; 32]);

    s.add_local_folder(folder("offered", "Offered Label", vec![peer_a]));
    s.set_folder_devices("labeled", vec![peer_a, peer_b]);
    s.set_folder_label("labeled", "My Labeled Share");
    s.set_folder_devices("unlabeled", vec![peer_a]);
    s.set_folder_devices("other_only", vec![peer_b]);

    let for_a = s.configured_folders_for_peer(&peer_a);
    let summary: Vec<(&str, &str)> = for_a
        .iter()
        .map(|f| (f.id.as_str(), f.label.as_str()))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("offered", "Offered Label"),
            ("labeled", "My Labeled Share"),
            ("unlabeled", "unlabeled"),
        ]
    );

    // Clearing the label falls back to the folder ID.
    s.set_folder_label("labeled", "");
    let for_b = s.configured_folders_for_peer(&peer_b);
    let summary_b: Vec<(&str, &str)> = for_b
        .iter()
        .map(|f| (f.id.as_str(), f.label.as_str()))
        .collect();
    assert_eq!(
        summary_b,
        vec![("labeled", "labeled"), ("other_only", "other_only")]
    );
}
