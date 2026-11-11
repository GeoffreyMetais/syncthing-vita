//! `ShareService` use cases with `ConfigRepository` persistence: share creation, device linking,
//! discovered folders and manual LAN pairing.

mod common;

use common::temp_test_dir;
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{FolderOfferService, ShareService};
use vita_syncthing_app::domain::AppState;
use vita_syncthing_app::infrastructure::ConfigRepository;

#[test]
fn test_share_service_and_config_repository_roundtrip() {
    let dir = temp_test_dir("share-service");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    let dev_id = DeviceId([0x42; 32]);
    ShareService::upsert_device(&mut state, &repo, dev_id, "MacBook", "192.168.1.10:22000");
    assert_eq!(state.devices.len(), 1);

    let idx = ShareService::create_share(
        &mut state,
        &repo,
        "vita-savedata",
        "Vita Save Data",
        "ux0:user/00/savedata",
    );
    assert_eq!(idx, 0);
    assert_eq!(state.shares.len(), 1);
    assert!(state.shares[0].enabled);
    assert!(state.shares[0].shared_devices.is_empty());
    assert!(state.pending_folder_offers.is_empty());

    // Link device explicitly to share
    let (_, _, added) =
        ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
    assert!(added);
    assert_eq!(state.shares[0].shared_devices, vec![dev_id.to_hex()]);
    assert_eq!(
        state.pending_folder_offers,
        vec!["vita-savedata".to_owned()]
    );

    // Toggle device off and back on
    let (_, _, added_off) =
        ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
    assert!(!added_off);
    assert!(state.shares[0].shared_devices.is_empty());

    let (_, _, added_again) =
        ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
    assert!(added_again);
    assert_eq!(state.shares[0].shared_devices, vec![dev_id.to_hex()]);

    // An accepted remote folder offer starts paused (enabled = false)
    FolderOfferService::receive(&mut state, &repo, dev_id, "remote-docs", "Remote Docs");
    assert_eq!(state.shares.len(), 1);
    FolderOfferService::accept(&mut state, &repo, 0);
    assert_eq!(state.shares.len(), 2);
    assert!(!state.shares[1].enabled);

    // Reload from disk via ConfigRepository
    let loaded_devs = repo.load_devices();
    let loaded_shares = repo.load_shares();
    assert_eq!(loaded_devs, state.devices);
    assert_eq!(loaded_shares, state.shares);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_passive_lan_discovery_does_not_auto_link_or_connect() {
    let dir = temp_test_dir("manual-lan-pairing");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    let random_lan_peer = DeviceId([0x77; 32]);
    ShareService::register_discovered_peer(
        &mut state,
        &repo,
        random_lan_peer,
        "LAN Peer",
        "192.168.1.99:22000",
    );

    // Discovered LAN peer is visible in UI list (`state.devices`) but NOT approved for automatic connection
    assert_eq!(state.devices.len(), 1);
    assert!(!state.has_configured_peers());
    assert!(state.approved_devices().is_empty());

    // Creating a share must NOT auto-link unapproved LAN peers
    let share_idx = ShareService::create_share(
        &mut state,
        &repo,
        "vita-savedata",
        "Vita Save Data",
        "ux0:user/00/savedata",
    );
    assert!(state.shares[share_idx].shared_devices.is_empty());
    assert!(!state.has_configured_peers());

    // Only when the user manually ticks the peer in ManageDevices does it become approved
    let (_, _, added) =
        ShareService::toggle_device_for_share(&mut state, &repo, share_idx, random_lan_peer)
            .unwrap();
    assert!(added);
    assert!(state.has_configured_peers());
    assert_eq!(state.approved_devices().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_delete_share_removes_config_and_pending_offer() {
    let dir = temp_test_dir("delete-share");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    let dev_id = DeviceId([0x24; 32]);
    ShareService::upsert_device(&mut state, &repo, dev_id, "Laptop", "192.168.1.20:22000");
    ShareService::create_share(&mut state, &repo, "a", "Share A", &format!("{}/a", dir));
    let b = ShareService::create_share(&mut state, &repo, "b", "Share B", &format!("{}/b", dir));
    ShareService::toggle_device_for_share(&mut state, &repo, b, dev_id).unwrap();
    assert!(state.pending_folder_offers.contains(&"b".to_owned()));

    let label = ShareService::delete_share(&mut state, &repo, b);
    assert_eq!(label.as_deref(), Some("Share B"));
    assert_eq!(state.shares.len(), 1);
    assert_eq!(state.shares[0].folder_id, "a");
    assert_eq!(state.selected_share_idx, 0);
    assert!(state.pending_folder_offers.is_empty());
    assert_eq!(repo.load_shares().len(), 1);
    assert!(ShareService::delete_share(&mut state, &repo, 5).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}
