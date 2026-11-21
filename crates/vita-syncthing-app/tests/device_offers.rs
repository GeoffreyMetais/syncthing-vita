//! Integration tests for incoming unknown-device offers (`DeviceOfferService`, `ModalInput`,
//! `ignored_devices.cfg` persistence, `SyncCoordinator` routing, and 5-language modal rendering).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc;

use syncthing_core::{DeviceId, SyncEvent};
use vita_syncthing_app::application::{
    DeviceOfferService, FolderOfferService, ModalInput, SyncCoordinator,
};
use vita_syncthing_app::domain::defaults::MAX_DEVICE_OFFERS;
use vita_syncthing_app::domain::{AppState, KnownDevice, Language, ModalDialog, UiText};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, WorkerMsg, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE,
};
use vita_syncthing_app::ui::VitaUiRenderer;

mod common;
use common::temp_test_dir;

fn dev(byte: u8) -> DeviceId {
    DeviceId([byte; 32])
}

fn empty_state() -> AppState {
    AppState::new(Vec::new(), Vec::new(), "/tmp")
}

#[test]
fn test_receive_prompt_and_accept_via_gamepad() {
    let dir = temp_test_dir("device-offer-accept");
    let repo = ConfigRepository::new(&dir);
    let mut state = empty_state();
    let peer = dev(0x11);

    DeviceOfferService::receive(&mut state, &repo, peer, "192.168.1.42", false);
    assert_eq!(state.device_offers.len(), 1);
    assert_eq!(state.device_offers[0].address, "192.168.1.42");
    assert!(!state.device_offers[0].via_relay);

    // Duplicate receive updates address without adding a second entry.
    DeviceOfferService::receive(&mut state, &repo, peer, "192.168.1.99", false);
    assert_eq!(state.device_offers.len(), 1);
    assert_eq!(state.device_offers[0].address, "192.168.1.99");

    DeviceOfferService::prompt_next(&mut state);
    assert_eq!(state.modal, ModalDialog::DeviceOffer { offer_idx: 0 });

    let mut input = InputFrame::default();
    input.pressed = BTN_CROSS;
    ModalInput::handle(&mut state, &repo, input);

    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.device_offers.is_empty());
    assert!(state.config_dirty);
    assert!(state.auto_reconnect_enabled);
    assert!(state
        .approved_devices()
        .iter()
        .any(|d| d.device_id == peer && d.address == "dynamic"));
    assert!(matches!(
        state.toast_msg.as_ref().map(|(t, _)| t),
        Some(UiText::DeviceOfferAccepted { .. })
    ));

    // Persisted in devices.cfg.
    let saved = repo.load_devices();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].device_id, peer);
    assert_eq!(saved[0].address, "dynamic");

    // Re-receiving an already-approved device is ignored.
    DeviceOfferService::receive(&mut state, &repo, peer, "192.168.1.99", false);
    assert!(state.device_offers.is_empty());
}

#[test]
fn test_decline_persists_ignored_device_and_clear_declined_resets_it() {
    let dir = temp_test_dir("device-offer-decline");
    let repo = ConfigRepository::new(&dir);
    let mut state = empty_state();
    let peer = dev(0x22);

    DeviceOfferService::receive(&mut state, &repo, peer, "", true);
    DeviceOfferService::prompt_next(&mut state);
    assert_eq!(state.modal, ModalDialog::DeviceOffer { offer_idx: 0 });

    let mut input = InputFrame::default();
    input.pressed = BTN_SQUARE;
    ModalInput::handle(&mut state, &repo, input);

    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.device_offers.is_empty());
    assert!(state.is_device_ignored(&peer));
    assert_eq!(repo.load_ignored_devices(), vec![peer]);
    assert!(matches!(
        state.toast_msg.as_ref().map(|(t, _)| t),
        Some(UiText::DeviceOfferDeclined { .. })
    ));

    // Re-receiving the declined device is suppressed.
    DeviceOfferService::receive(&mut state, &repo, peer, "192.168.1.10", false);
    assert!(state.device_offers.is_empty());

    // Clearing declined offers in Settings clears ignored_devices too.
    let cleared = FolderOfferService::clear_declined(&mut state, &repo);
    assert_eq!(cleared, 1);
    assert!(state.ignored_devices.is_empty());
    assert!(repo.load_ignored_devices().is_empty());

    // Now the device can be offered again.
    DeviceOfferService::receive(&mut state, &repo, peer, "192.168.1.10", false);
    assert_eq!(state.device_offers.len(), 1);
}

#[test]
fn test_later_snoozes_device_offer_and_queue_is_capped() {
    let dir = temp_test_dir("device-offer-later");
    let repo = ConfigRepository::new(&dir);
    let mut state = empty_state();

    for i in 1..=(MAX_DEVICE_OFFERS + 4) {
        DeviceOfferService::receive(&mut state, &repo, dev(i as u8), "10.0.0.5", false);
    }
    assert_eq!(state.device_offers.len(), MAX_DEVICE_OFFERS);

    DeviceOfferService::prompt_next(&mut state);
    assert_eq!(state.modal, ModalDialog::DeviceOffer { offer_idx: 0 });

    let mut input = InputFrame::default();
    input.pressed = BTN_CIRCLE;
    ModalInput::handle(&mut state, &repo, input);

    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.device_offers[0].snoozed);
    assert_eq!(state.next_device_offer_to_prompt(), Some(1));
}

#[test]
fn test_sync_coordinator_routes_device_offered_and_preserves_addr_on_inbound_connect() {
    let dir = temp_test_dir("device-offer-coord");
    let repo = ConfigRepository::new(&dir);
    let existing_peer = dev(0x33);
    let unknown_peer = dev(0x44);
    let mut state = AppState::new(
        Vec::new(),
        vec![KnownDevice {
            device_id: existing_peer,
            name: "Desktop".to_owned(),
            address: "192.168.1.20:22000".to_owned(),
        }],
        "/tmp",
    );

    let (tx, rx) = mpsc::channel();
    let mut rx_opt = Some(rx);
    tx.send(WorkerMsg::DeviceOffered {
        device_id: unknown_peer,
        address: "192.168.1.77".to_owned(),
        via_relay: false,
    })
    .unwrap();
    tx.send(WorkerMsg::InboundSyncEvent(SyncEvent::PeerConnected {
        peer_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), 54321),
        peer_device_id: existing_peer,
        peer_name: "Desktop".to_owned(),
        client_version: "v1.28.0".to_owned(),
    }))
    .unwrap();

    assert!(SyncCoordinator::process_worker_messages(
        &mut state,
        &repo,
        &mut rx_opt
    ));
    assert_eq!(state.device_offers.len(), 1);
    assert_eq!(state.device_offers[0].device_id, unknown_peer);
    // Ephemeral inbound source port 54321 must NOT overwrite configured 192.168.1.20:22000.
    assert_eq!(state.devices[0].address, "192.168.1.20:22000");
}

#[test]
fn test_device_offer_modal_renders_in_all_languages_and_declares_touch_buttons() {
    let dir = temp_test_dir("device-offer-render");
    let repo = ConfigRepository::new(&dir);
    let mut state = empty_state();
    DeviceOfferService::receive(&mut state, &repo, dev(0x55), "192.168.1.50", false);
    DeviceOfferService::receive(&mut state, &repo, dev(0x66), "", true);
    DeviceOfferService::prompt_next(&mut state);

    let mut renderer = VitaUiRenderer::new();
    for lang in Language::ALL {
        state.language = lang;
        renderer.render(&mut state, 1);
        assert!(
            !state.hit_map.hotspots().is_empty(),
            "DeviceOffer modal must declare touch hotspots in {lang:?}"
        );
    }
}
