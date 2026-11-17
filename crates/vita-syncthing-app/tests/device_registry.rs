//! `DeviceRegistry`: LAN peers discovered passively (bounded list), devices upserted after a TLS
//! session proved their ID (Short ID placeholder upgrade) and devices added from user input.

mod common;

use common::temp_test_dir;
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, ShareService};
use vita_syncthing_app::domain::defaults::MAX_DISCOVERED_PEERS;
use vita_syncthing_app::domain::{AppState, UiText};
use vita_syncthing_app::infrastructure::ConfigRepository;

fn full(byte: u8) -> DeviceId {
    DeviceId([byte; 32])
}

/// A Short ID placeholder: the first 7 characters of `full`, as typed by the user.
fn short_of(full: DeviceId) -> DeviceId {
    let placeholder = DeviceId::parse_user_id(&full.short()).unwrap();
    assert!(placeholder.is_short_placeholder());
    placeholder
}

fn address_of(state: &AppState, id: DeviceId) -> String {
    state
        .devices
        .iter()
        .find(|x| x.device_id == id)
        .unwrap()
        .address
        .clone()
}

fn setup(suffix: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(suffix);
    let repo = ConfigRepository::new(&dir);
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    (dir, repo, state)
}

#[test]
fn discovered_peer_without_name_gets_a_placeholder_label() {
    let (dir, _repo, mut state) = setup("registry-discovered-label");
    let id = full(0x10);

    DeviceRegistry::register_discovered_peer(&mut state, id, "", "192.168.1.10:22000");
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].name, format!("LAN Peer ({})", id.short()));
    assert_eq!(state.devices[0].address, "192.168.1.10:22000");
    assert_eq!(state.unpaired_lan_peers, vec![id]);

    // "LAN Peer" is the placeholder name sent by the discovery layer: same label.
    let other = full(0x11);
    DeviceRegistry::register_discovered_peer(&mut state, other, "LAN Peer", "");
    assert_eq!(
        state.devices[1].name,
        format!("LAN Peer ({})", other.short())
    );
    assert!(state.is_unpaired_lan_peer(&other));

    // A real name is kept as is.
    let named = full(0x12);
    DeviceRegistry::register_discovered_peer(&mut state, named, "Office NAS", "10.0.0.2:22000");
    assert_eq!(state.devices[2].name, "Office NAS");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rediscovered_peer_refreshes_name_and_address_only_with_real_values() {
    let (dir, _repo, mut state) = setup("registry-discovered-refresh");
    let id = full(0x20);
    DeviceRegistry::register_discovered_peer(&mut state, id, "Laptop", "10.0.0.5:22000");

    // Empty name, placeholder name and "dynamic" address never overwrite what is known.
    DeviceRegistry::register_discovered_peer(&mut state, id, "", "dynamic");
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].name, "Laptop");
    assert_eq!(state.devices[0].address, "10.0.0.5:22000");

    DeviceRegistry::register_discovered_peer(&mut state, id, "LAN Peer", "");
    assert_eq!(state.devices[0].name, "Laptop");
    assert_eq!(state.devices[0].address, "10.0.0.5:22000");

    // Real values refresh the entry in place.
    DeviceRegistry::register_discovered_peer(&mut state, id, "Laptop 2", "10.0.0.6:22000");
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].name, "Laptop 2");
    assert_eq!(state.devices[0].address, "10.0.0.6:22000");
    assert_eq!(state.unpaired_lan_peers, vec![id]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn discovered_peers_are_listed_up_to_the_bound_only() {
    let (dir, _repo, mut state) = setup("registry-discovered-bound");
    for i in 0..MAX_DISCOVERED_PEERS {
        DeviceRegistry::register_discovered_peer(&mut state, full(i as u8 + 1), "", "");
    }
    assert_eq!(state.unpaired_lan_peers.len(), MAX_DISCOVERED_PEERS);
    assert_eq!(state.devices.len(), MAX_DISCOVERED_PEERS);

    // One more unknown peer is refused: the list stays bounded.
    let extra = full(0xF0);
    DeviceRegistry::register_discovered_peer(&mut state, extra, "", "");
    assert_eq!(state.unpaired_lan_peers.len(), MAX_DISCOVERED_PEERS);
    assert!(!state.devices.iter().any(|d| d.device_id == extra));

    // A peer already listed is still refreshed when the list is full.
    DeviceRegistry::register_discovered_peer(&mut state, full(1), "Renamed", "");
    assert_eq!(state.devices[0].name, "Renamed");
    assert_eq!(state.devices.len(), MAX_DISCOVERED_PEERS);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upsert_registers_a_new_device_with_a_label_and_persists_it() {
    let (dir, repo, mut state) = setup("registry-upsert-new");
    let id = full(0x30);

    DeviceRegistry::upsert_device(&mut state, &repo, id, "", "10.1.1.1:22000");
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].name, format!("Device {}", id.short()));
    assert_eq!(state.devices[0].address, "10.1.1.1:22000");
    assert_eq!(repo.load_devices(), state.devices);

    let named = full(0x31);
    DeviceRegistry::upsert_device(&mut state, &repo, named, "Phone", "dynamic");
    assert_eq!(state.devices[1].name, "Phone");
    assert_eq!(state.devices[1].address, "dynamic");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upsert_of_a_known_device_keeps_name_and_address_when_not_given() {
    let (dir, repo, mut state) = setup("registry-upsert-known");
    let id = full(0x40);
    DeviceRegistry::upsert_device(&mut state, &repo, id, "Desk", "10.2.2.2:22000");

    DeviceRegistry::upsert_device(&mut state, &repo, id, "", "dynamic");
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].name, "Desk");
    assert_eq!(state.devices[0].address, "10.2.2.2:22000");

    DeviceRegistry::upsert_device(&mut state, &repo, id, "LAN Peer", "");
    assert_eq!(state.devices[0].name, "Desk");

    DeviceRegistry::upsert_device(&mut state, &repo, id, "Desk v2", "10.2.2.3:22000");
    assert_eq!(state.devices[0].name, "Desk v2");
    assert_eq!(state.devices[0].address, "10.2.2.3:22000");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upsert_stops_treating_a_proven_device_as_an_unpaired_lan_peer() {
    let (dir, repo, mut state) = setup("registry-upsert-unpair");
    let id = full(0x50);
    DeviceRegistry::register_discovered_peer(&mut state, id, "", "");
    assert!(state.is_unpaired_lan_peer(&id));

    DeviceRegistry::upsert_device(&mut state, &repo, id, "Proven", "10.3.3.3:22000");
    assert!(!state.is_unpaired_lan_peer(&id));
    assert_eq!(state.devices.len(), 1);
    assert!(state.has_configured_peers());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upsert_with_the_proven_id_upgrades_a_short_placeholder_everywhere() {
    let (dir, repo, mut state) = setup("registry-upsert-upgrade");
    let proven = full(0x60);
    let placeholder = short_of(proven);

    DeviceRegistry::upsert_device(&mut state, &repo, placeholder, "Peer X (Short ID)", "");
    assert_eq!(state.devices[0].device_id, placeholder);
    ShareService::create_share(&mut state, &repo, "docs", "Docs", "ux0:docs");
    ShareService::toggle_device_for_share(&mut state, &repo, 0, placeholder).unwrap();

    // The TLS session proved the full ID: the placeholder becomes that ID everywhere.
    DeviceRegistry::upsert_device(&mut state, &repo, proven, "", "10.4.4.4:22000");
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].device_id, proven);
    assert_eq!(state.devices[0].name, format!("Device {}", proven.short()));
    assert_eq!(state.devices[0].address, "10.4.4.4:22000");
    assert_eq!(state.shares[0].shared_devices, vec![proven]);
    assert_eq!(repo.load_shares()[0].shared_devices, vec![proven]);
    assert_eq!(repo.load_devices()[0].device_id, proven);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upgraded_placeholder_keeps_a_custom_name_and_takes_a_given_one() {
    let (dir, repo, mut state) = setup("registry-upsert-custom-name");
    let proven = full(0x61);
    let placeholder = short_of(proven);
    DeviceRegistry::upsert_device(&mut state, &repo, placeholder, "Office NAS", "");

    // No (Short ID) suffix and no name given: the custom name is kept.
    DeviceRegistry::upsert_device(&mut state, &repo, proven, "", "");
    assert_eq!(state.devices[0].device_id, proven);
    assert_eq!(state.devices[0].name, "Office NAS");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upserting_a_placeholder_again_does_not_rename_it() {
    let (dir, repo, mut state) = setup("registry-upsert-same-placeholder");
    let placeholder = short_of(full(0x62));
    DeviceRegistry::upsert_device(&mut state, &repo, placeholder, "Peer ABC (Short ID)", "");

    DeviceRegistry::upsert_device(&mut state, &repo, placeholder, "", "");
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].device_id, placeholder);
    assert_eq!(state.devices[0].name, "Peer ABC (Short ID)");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_different_full_id_never_upgrades_another_device() {
    let (dir, repo, mut state) = setup("registry-upsert-no-upgrade");
    let a = full(0x63);
    let b = full(0x64);
    DeviceRegistry::upsert_device(&mut state, &repo, a, "A", "");
    DeviceRegistry::upsert_device(&mut state, &repo, b, "B", "");
    assert_eq!(state.devices.len(), 2);
    assert_eq!(state.devices[0].device_id, a);
    assert_eq!(state.devices[0].name, "A");
    assert_eq!(state.devices[1].device_id, b);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_by_full_id_registers_a_device_linked_to_nothing() {
    let (dir, repo, mut state) = setup("registry-add-full");
    let id = full(0x70);
    let input = id.to_syncthing_string();

    let (resolved, name, placeholder) =
        DeviceRegistry::add_device_by_input(&mut state, &repo, &input, None).unwrap();
    assert_eq!(resolved, id);
    assert_eq!(name, format!("Device {}", id.short()));
    assert!(!placeholder);
    assert_eq!(state.devices[0].address, "dynamic");
    assert!(state.shares.is_empty());
    assert!(state.config_dirty);
    assert!(state.auto_reconnect_enabled);
    assert_eq!(repo.load_devices(), state.devices);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_by_input_parses_the_optional_address_part() {
    let (dir, repo, mut state) = setup("registry-add-address");
    let a = full(0x71);
    let b = full(0x72);
    let c = full(0x73);
    let d = full(0x74);

    // Host without port: the Syncthing default port is appended.
    let input_a = format!("  {}@ 192.168.1.20  ", a.to_syncthing_string());
    DeviceRegistry::add_device_by_input(&mut state, &repo, &input_a, None).unwrap();
    // Host with port: kept as typed.
    let input_b = format!("{}@192.168.1.21:2222", b.to_syncthing_string());
    DeviceRegistry::add_device_by_input(&mut state, &repo, &input_b, None).unwrap();
    // Empty address after '@': the device stays dynamic.
    let input_c = format!("{}@   ", c.to_syncthing_string());
    DeviceRegistry::add_device_by_input(&mut state, &repo, &input_c, None).unwrap();
    // No '@' at all: dynamic.
    let input_d = d.to_syncthing_string();
    DeviceRegistry::add_device_by_input(&mut state, &repo, &input_d, None).unwrap();

    assert_eq!(address_of(&state, a), "192.168.1.20:22000");
    assert_eq!(address_of(&state, b), "192.168.1.21:2222");
    assert_eq!(address_of(&state, c), "dynamic");
    assert_eq!(address_of(&state, d), "dynamic");

    // Adding a known device with an address updates it; without one the address is kept.
    let known = format!("{}@10.9.9.9", a.to_syncthing_string());
    DeviceRegistry::add_device_by_input(&mut state, &repo, &known, None).unwrap();
    assert_eq!(address_of(&state, a), "10.9.9.9:22000");
    DeviceRegistry::add_device_by_input(&mut state, &repo, &a.to_syncthing_string(), None).unwrap();
    assert_eq!(address_of(&state, a), "10.9.9.9:22000");
    assert_eq!(state.devices.len(), 4);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_rejects_invalid_input_and_the_own_device_id() {
    let (dir, repo, mut state) = setup("registry-add-rejects");
    let own = full(0x75);
    state.vita_device_id = own.to_syncthing_string();

    let err = DeviceRegistry::add_device_by_input(&mut state, &repo, "ab", None).unwrap_err();
    assert!(matches!(err, UiText::InvalidDeviceId(_)));
    assert!(state.devices.is_empty());

    let err =
        DeviceRegistry::add_device_by_input(&mut state, &repo, &own.to_syncthing_string(), None)
            .unwrap_err();
    assert_eq!(err, UiText::OwnDeviceId);
    assert!(state.devices.is_empty());
    assert!(!state.config_dirty);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_of_a_short_id_creates_a_placeholder_and_keeps_its_label_on_repeat() {
    let (dir, repo, mut state) = setup("registry-add-short");
    let proven = full(0x76);
    let short_text = proven.short();

    let (resolved, name, placeholder) =
        DeviceRegistry::add_device_by_input(&mut state, &repo, &short_text, None).unwrap();
    assert!(placeholder);
    assert!(resolved.is_short_placeholder());
    assert_eq!(name, format!("Peer {} (Short ID)", proven.short()));
    assert_eq!(state.devices[0].name, name);
    assert_eq!(state.devices[0].address, "dynamic");

    // Typing the same Short ID again must not rename the placeholder.
    let (again, name_again, still) =
        DeviceRegistry::add_device_by_input(&mut state, &repo, &short_text, None).unwrap();
    assert_eq!(again, resolved);
    assert!(still);
    assert_eq!(name_again, name);
    assert_eq!(state.devices.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_of_a_full_id_upgrades_a_matching_short_placeholder_and_relinks_shares() {
    let (dir, repo, mut state) = setup("registry-add-upgrade");
    let proven = full(0x77);
    let placeholder = short_of(proven);
    DeviceRegistry::add_device_by_input(&mut state, &repo, &placeholder.syncthing_prefix(), None)
        .unwrap();
    ShareService::create_share(&mut state, &repo, "music", "Music", "ux0:music");
    ShareService::toggle_device_for_share(&mut state, &repo, 0, placeholder).unwrap();

    let (resolved, name, is_placeholder) = DeviceRegistry::add_device_by_input(
        &mut state,
        &repo,
        &proven.to_syncthing_string(),
        Some(0),
    )
    .unwrap();
    assert_eq!(resolved, proven);
    assert!(!is_placeholder);
    assert_eq!(name, format!("Device {}", proven.short()));
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].device_id, proven);
    assert_eq!(state.shares[0].shared_devices, vec![proven]);
    assert_eq!(repo.load_shares()[0].shared_devices, vec![proven]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_links_the_device_to_the_share_and_offers_it_when_enabled() {
    let (dir, repo, mut state) = setup("registry-add-link");
    ShareService::create_share(&mut state, &repo, "photos", "Photos", "ux0:picture");
    let id = full(0x78);
    let input = id.to_syncthing_string();

    DeviceRegistry::add_device_by_input(&mut state, &repo, &input, Some(0)).unwrap();
    assert_eq!(state.shares[0].shared_devices, vec![id]);
    assert_eq!(state.pending_folder_offers, vec!["photos".to_owned()]);

    // Linking again does not duplicate the entry.
    DeviceRegistry::add_device_by_input(&mut state, &repo, &input, Some(0)).unwrap();
    assert_eq!(state.shares[0].shared_devices, vec![id]);

    // A paused share is linked but gets no folder offer.
    ShareService::toggle_selected_share(&mut state, &repo);
    state.pending_folder_offers.clear();
    let other = full(0x79);
    DeviceRegistry::add_device_by_input(&mut state, &repo, &other.to_syncthing_string(), Some(0))
        .unwrap();
    assert_eq!(state.shares[0].shared_devices, vec![id, other]);
    assert!(state.pending_folder_offers.is_empty());

    // A share index that no longer exists is ignored, the device is still added.
    let third = full(0x7A);
    DeviceRegistry::add_device_by_input(&mut state, &repo, &third.to_syncthing_string(), Some(9))
        .unwrap();
    assert!(state.devices.iter().any(|d| d.device_id == third));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_removes_the_device_from_the_unpaired_lan_peers() {
    let (dir, repo, mut state) = setup("registry-add-unpair");
    let id = full(0x7B);
    DeviceRegistry::register_discovered_peer(&mut state, id, "", "");
    assert!(state.is_unpaired_lan_peer(&id));

    DeviceRegistry::add_device_by_input(&mut state, &repo, &id.to_syncthing_string(), None)
        .unwrap();
    assert!(!state.is_unpaired_lan_peer(&id));
    assert!(state.has_configured_peers());

    let _ = std::fs::remove_dir_all(&dir);
}
