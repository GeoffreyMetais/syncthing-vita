//! `ShareService` use cases with `ConfigRepository` persistence: share creation, device linking,
//! discovered folders and manual LAN pairing.

mod common;

use common::temp_test_dir;
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, FolderOfferService, ShareService};
use vita_syncthing_app::domain::AppState;
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
