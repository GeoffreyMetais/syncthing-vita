//! `ShareService` use cases with `ConfigRepository` persistence: share creation, device linking,
//! discovered folders and manual LAN pairing.

mod common;

use common::temp_test_dir;
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, FolderOfferService, ShareService};
use vita_syncthing_app::domain::{AppState, UiText};
use vita_syncthing_app::infrastructure::ConfigRepository;

#[test]
fn test_share_service_and_config_repository_roundtrip() {
    let dir = temp_test_dir("share-service");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    let dev_id = DeviceId([0x42; 32]);
    DeviceRegistry::upsert_device(&mut state, &repo, dev_id, "MacBook", "192.168.1.10:22000");
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
    assert_eq!(state.shares[0].shared_devices, vec![dev_id]);
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
    assert_eq!(state.shares[0].shared_devices, vec![dev_id]);

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
    DeviceRegistry::register_discovered_peer(
        &mut state,
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
    DeviceRegistry::upsert_device(&mut state, &repo, dev_id, "Laptop", "192.168.1.20:22000");
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

#[test]
fn test_sync_peers_excludes_devices_without_active_share() {
    let dir = temp_test_dir("sync-peers-active-share");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    let unlinked = DeviceId([0x11; 32]);
    let paused_only = DeviceId([0x22; 32]);
    let active = DeviceId([0x33; 32]);

    DeviceRegistry::upsert_device(
        &mut state,
        &repo,
        unlinked,
        "Mac Deezer",
        "192.168.1.50:22000",
    );
    DeviceRegistry::upsert_device(
        &mut state,
        &repo,
        paused_only,
        "Old PC",
        "192.168.1.51:22000",
    );
    DeviceRegistry::upsert_device(&mut state, &repo, active, "NAS", "192.168.1.52:22000");

    // With no shares yet, approved devices are eligible so folder offers can arrive.
    assert_eq!(state.sync_peers().len(), 3);

    // Once shares exist, only devices linked to at least one enabled share are dialed.
    let s_active =
        ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    ShareService::toggle_device_for_share(&mut state, &repo, s_active, active);

    let s_paused =
        ShareService::create_share(&mut state, &repo, "roms", "Roms", &format!("{dir}/roms"));
    ShareService::toggle_device_for_share(&mut state, &repo, s_paused, paused_only);
    state.shares[s_paused].enabled = false;

    let peers = state.sync_peers();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].device_id, active);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A LAN announcement is not authenticated: it must not turn a Short ID placeholder into a
/// full ID (only a TLS session does), and it cannot fill the device list without bound.
#[test]
fn test_lan_announcement_never_upgrades_a_short_id_placeholder() {
    use vita_syncthing_app::domain::defaults::MAX_DISCOVERED_PEERS;
    let dir = temp_test_dir("announce-no-upgrade");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    let real = DeviceId([0x5A; 32]);
    let prefix: String = real.to_syncthing_string().chars().take(7).collect();
    let (placeholder, _, is_short) =
        DeviceRegistry::add_device_by_input(&mut state, &repo, &prefix, None).unwrap();
    assert!(is_short);

    // Someone announces an ID sharing the prefix.
    let mut forged = real;
    forged.0[20] ^= 0xFF;
    assert!(placeholder.matches_peer(&forged));
    DeviceRegistry::register_discovered_peer(&mut state, forged, "LAN Peer", "192.168.1.66:22000");
    assert_eq!(state.devices[0].device_id, placeholder);

    // The real device proves its identity over TLS: the placeholder becomes its full ID.
    DeviceRegistry::upsert_device(&mut state, &repo, real, "NAS", "192.168.1.10:22000");
    assert_eq!(state.devices[0].device_id, real);

    for i in 0..(MAX_DISCOVERED_PEERS as u8 + 10) {
        let mut id = [i; 32];
        id[0] = 0xEE;
        DeviceRegistry::register_discovered_peer(
            &mut state,
            DeviceId(id),
            "LAN Peer",
            "10.0.0.1:1",
        );
    }
    assert_eq!(state.unpaired_lan_peers.len(), MAX_DISCOVERED_PEERS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_share_on_an_existing_folder_updates_it_and_offers_it_when_linked() {
    let dir = temp_test_dir("share-service-recreate");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let dev = DeviceId([0x31; 32]);
    DeviceRegistry::upsert_device(&mut state, &repo, dev, "Mac", "");

    let first = format!("{}/first", dir);
    ShareService::create_share(&mut state, &repo, "docs", "Docs", &first);
    ShareService::toggle_selected_share(&mut state, &repo); // paused, nothing linked yet
    assert!(!state.shares[0].enabled);
    state.pending_folder_offers.clear();

    // Re-creating an existing folder updates it in place and re-enables it. No device is linked,
    // so no folder offer is queued.
    let second = format!("{}/second", dir);
    let idx = ShareService::create_share(&mut state, &repo, "docs", "Docs v2", &second);
    assert_eq!(idx, 0);
    assert_eq!(state.shares.len(), 1);
    assert_eq!(state.shares[0].label, "Docs v2");
    assert_eq!(state.shares[0].target_path, second);
    assert!(state.shares[0].enabled);
    assert!(state.pending_folder_offers.is_empty());
    assert!(
        std::path::Path::new(&second).is_dir(),
        "target directory is created"
    );
    assert_eq!(repo.load_shares()[0].target_path, second);

    // Once a device is linked, re-creating the share queues its folder offer.
    ShareService::toggle_device_for_share(&mut state, &repo, 0, dev).unwrap();
    state.pending_folder_offers.clear();
    ShareService::create_share(&mut state, &repo, "docs", "Docs v3", &second);
    assert_eq!(state.pending_folder_offers, vec!["docs".to_owned()]);
    assert_eq!(state.shares[0].shared_devices, vec![dev]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_selected_share_target_trims_saves_and_returns_to_overview() {
    let dir = temp_test_dir("share-service-target");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::create_share(&mut state, &repo, "a", "A", &format!("{}/a", dir));
    ShareService::create_share(&mut state, &repo, "b", "B", &format!("{}/b", dir));
    state.config_dirty = false;
    state.active_tab = vita_syncthing_app::domain::ActiveTab::FolderPicker;

    // Selected share is `b` (the last created one).
    let chosen = format!("{}/moved/", dir);
    ShareService::update_selected_share_target(&mut state, &repo, &chosen);

    let expected = format!("{}/moved", dir);
    assert_eq!(
        state.shares[1].target_path, expected,
        "trailing slash is trimmed"
    );
    assert_eq!(state.shares[0].target_path, format!("{}/a", dir));
    assert!(state.config_dirty);
    assert_eq!(repo.load_shares()[1].target_path, expected);
    assert!(std::path::Path::new(&expected).is_dir());
    assert_eq!(
        state.toast_msg.as_ref().map(|(msg, _)| msg.clone()),
        Some(UiText::SavedIn {
            path: expected.clone()
        })
    );
    assert_eq!(
        state.active_tab,
        vita_syncthing_app::domain::ActiveTab::Dashboard
    );
    assert!(state
        .logs
        .iter()
        .any(|l| l == &format!("Target directory updated for b -> {}", expected)));

    // Without a selected share nothing changes and the tab stays.
    state.selected_share_idx = 7;
    state.config_dirty = false;
    state.active_tab = vita_syncthing_app::domain::ActiveTab::FolderPicker;
    ShareService::update_selected_share_target(&mut state, &repo, "ux0:elsewhere");
    assert!(!state.config_dirty);
    assert_eq!(
        state.active_tab,
        vita_syncthing_app::domain::ActiveTab::FolderPicker
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_selected_share_pauses_and_resumes_with_the_matching_offers_and_messages() {
    let dir = temp_test_dir("share-service-toggle");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let dev = DeviceId([0x32; 32]);
    DeviceRegistry::upsert_device(&mut state, &repo, dev, "PC", "");
    ShareService::create_share(&mut state, &repo, "pics", "Pics", &format!("{}/p", dir));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, dev).unwrap();
    state.pending_folder_offers.clear();
    state.config_dirty = false;

    // Pausing a share with linked devices queues nothing and reports "Paused".
    ShareService::toggle_selected_share(&mut state, &repo);
    assert!(!state.shares[0].enabled);
    assert!(state.pending_folder_offers.is_empty());
    assert!(state.config_dirty);
    assert!(state.auto_reconnect_enabled);
    assert!(!repo.load_shares()[0].enabled);
    assert_eq!(
        state.toast_msg.as_ref().map(|(msg, _)| msg.clone()),
        Some(UiText::SharePaused {
            id: "pics".to_owned()
        })
    );
    assert!(state.logs.iter().any(|l| l == "Share pics is now Paused"));

    // Resuming a share with linked devices queues its offer and reports "Active".
    state.config_dirty = false;
    ShareService::toggle_selected_share(&mut state, &repo);
    assert!(state.shares[0].enabled);
    assert_eq!(state.pending_folder_offers, vec!["pics".to_owned()]);
    assert!(state.config_dirty);
    assert_eq!(
        state.toast_msg.as_ref().map(|(msg, _)| msg.clone()),
        Some(UiText::ShareActive {
            id: "pics".to_owned()
        })
    );
    assert!(state.logs.iter().any(|l| l == "Share pics is now Active"));

    // Resuming a share without any linked device queues nothing.
    ShareService::create_share(&mut state, &repo, "empty", "Empty", &format!("{}/e", dir));
    state.pending_folder_offers.clear();
    ShareService::toggle_selected_share(&mut state, &repo); // pause `empty`
    ShareService::toggle_selected_share(&mut state, &repo); // resume it
    assert!(state.shares[1].enabled);
    assert!(state.pending_folder_offers.is_empty());

    // No selected share: nothing happens.
    state.selected_share_idx = 9;
    state.config_dirty = false;
    ShareService::toggle_selected_share(&mut state, &repo);
    assert!(!state.config_dirty);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_device_reports_the_known_device_name_and_the_link_state() {
    let dir = temp_test_dir("share-service-toggle-name");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let laptop = DeviceId([0x33; 32]);
    let phone = DeviceId([0x34; 32]);
    let unknown = DeviceId([0x35; 32]);
    DeviceRegistry::upsert_device(&mut state, &repo, laptop, "Laptop", "");
    DeviceRegistry::upsert_device(&mut state, &repo, phone, "Phone", "");
    ShareService::create_share(&mut state, &repo, "s", "S", &format!("{}/s", dir));

    let (label, name, added) =
        ShareService::toggle_device_for_share(&mut state, &repo, 0, phone).unwrap();
    assert_eq!((label.as_str(), name.as_str(), added), ("S", "Phone", true));

    let (_, name, added) =
        ShareService::toggle_device_for_share(&mut state, &repo, 0, laptop).unwrap();
    assert_eq!((name.as_str(), added), ("Laptop", true));

    // A device unknown to the registry is named by its short ID.
    let (_, name, _) =
        ShareService::toggle_device_for_share(&mut state, &repo, 0, unknown).unwrap();
    assert_eq!(name, unknown.short());

    // Unlinking reports `added == false`; an unknown share index yields nothing.
    let (_, _, added) = ShareService::toggle_device_for_share(&mut state, &repo, 0, phone).unwrap();
    assert!(!added);
    assert!(ShareService::toggle_device_for_share(&mut state, &repo, 4, phone).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn linking_a_discovered_peer_only_clears_that_peer_from_the_unpaired_list() {
    let dir = temp_test_dir("share-service-unpair");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let a = DeviceId([0x36; 32]);
    let b = DeviceId([0x37; 32]);
    DeviceRegistry::register_discovered_peer(&mut state, a, "", "");
    DeviceRegistry::register_discovered_peer(&mut state, b, "", "");
    ShareService::create_share(&mut state, &repo, "s", "S", &format!("{}/s", dir));

    ShareService::toggle_device_for_share(&mut state, &repo, 0, a).unwrap();
    assert!(!state.is_unpaired_lan_peer(&a));
    assert!(
        state.is_unpaired_lan_peer(&b),
        "other discovered peers stay unpaired"
    );

    // Unlinking the only share of `a`: it is no longer approved, so it is listed as discovered again.
    ShareService::toggle_device_for_share(&mut state, &repo, 0, a).unwrap();
    assert!(state.is_unpaired_lan_peer(&a));
    assert!(state.is_unpaired_lan_peer(&b));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unlinking_a_device_still_linked_to_another_share_keeps_it_approved() {
    let dir = temp_test_dir("share-service-unlink-elsewhere");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let dev = DeviceId([0x38; 32]);
    DeviceRegistry::upsert_device(&mut state, &repo, dev, "Shared", "");
    ShareService::create_share(&mut state, &repo, "one", "One", &format!("{}/1", dir));
    ShareService::create_share(&mut state, &repo, "two", "Two", &format!("{}/2", dir));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, dev).unwrap();
    ShareService::toggle_device_for_share(&mut state, &repo, 1, dev).unwrap();

    ShareService::toggle_device_for_share(&mut state, &repo, 0, dev).unwrap();
    assert!(state.shares[1].is_linked_to(&dev));
    assert!(
        !state.is_unpaired_lan_peer(&dev),
        "still linked to share two"
    );
    assert!(state.has_configured_peers());

    ShareService::toggle_device_for_share(&mut state, &repo, 1, dev).unwrap();
    assert!(
        state.is_unpaired_lan_peer(&dev),
        "linked to no share any more"
    );
    assert!(!state.has_configured_peers());

    let _ = std::fs::remove_dir_all(&dir);
}
