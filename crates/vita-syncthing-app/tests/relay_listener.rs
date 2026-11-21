use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use syncthing_core::{DeviceId, DeviceIdentity};
use vita_syncthing_app::infrastructure::relay_listener::{
    get_shared_relay_uri, is_supported_relay_addr, set_shared_relay_uri, triage_invitation,
    InvitationDecision, SharedRelayUri,
};
use vita_syncthing_app::infrastructure::session_guard::{ConnectionSlots, SessionClaims};

fn id_from_seed(byte: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[byte; 32]).unwrap().device_id
}

#[test]
fn triage_rejects_malformed_from_without_taking_slot() {
    let self_id = id_from_seed(1);
    let slots = Arc::new(ConnectionSlots::new(2));

    let decision = triage_invitation(&[1, 2, 3], self_id, |_| true, |_| false, &slots);
    assert!(matches!(decision, InvitationDecision::Malformed));
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn triage_rejects_self_id_before_approval_check() {
    let self_id = id_from_seed(2);
    let slots = Arc::new(ConnectionSlots::new(2));

    let decision = triage_invitation(&self_id.0, self_id, |_| false, |_| false, &slots);
    assert!(matches!(decision, InvitationDecision::SelfId));
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn triage_reports_unknown_device_without_taking_slot() {
    let self_id = id_from_seed(3);
    let unknown_id = id_from_seed(4);
    let slots = Arc::new(ConnectionSlots::new(2));

    let decision = triage_invitation(&unknown_id.0, self_id, |_| false, |_| false, &slots);
    assert!(matches!(decision, InvitationDecision::Unknown(id) if id == unknown_id));
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn triage_detects_already_connected_via_short_id_claim() {
    let self_id = id_from_seed(5);
    let peer_full = id_from_seed(6);
    let peer_short = DeviceId::parse_user_id(&peer_full.short()).unwrap();

    let claims = Arc::new(SessionClaims::new(4));
    let _claim = claims.try_claim(peer_short).unwrap();
    let slots = Arc::new(ConnectionSlots::new(2));

    let decision = triage_invitation(
        &peer_full.0,
        self_id,
        |id| peer_short.matches_peer(id),
        |id| claims.is_claimed(id),
        &slots,
    );
    assert!(matches!(decision, InvitationDecision::AlreadyConnected(id) if id == peer_full));
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn triage_returns_busy_when_slots_exhausted() {
    let self_id = id_from_seed(7);
    let peer_id = id_from_seed(8);
    let slots = Arc::new(ConnectionSlots::new(1));
    let _occupied = slots.try_take().unwrap();

    let decision = triage_invitation(&peer_id.0, self_id, |_| true, |_| false, &slots);
    assert!(matches!(decision, InvitationDecision::Busy(id) if id == peer_id));
    assert_eq!(slots.in_use(), 1);
}

#[test]
fn triage_joins_approved_unclaimed_peer_and_releases_slot_on_drop() {
    let self_id = id_from_seed(9);
    let peer_id = id_from_seed(10);
    let slots = Arc::new(ConnectionSlots::new(2));

    let decision = triage_invitation(&peer_id.0, self_id, |_| true, |_| false, &slots);
    match decision {
        InvitationDecision::Join(id, slot) => {
            assert_eq!(id, peer_id);
            assert_eq!(slots.in_use(), 1);
            drop(slot);
        }
        other => panic!("expected Join, got {other:?}"),
    }
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn shared_relay_uri_transitions_cleanly() {
    let shared: SharedRelayUri = Arc::new(Mutex::new(None));
    assert_eq!(get_shared_relay_uri(&shared), None);

    let uri = "relay://198.51.100.1:22067/?id=ABCDEFG".to_owned();
    set_shared_relay_uri(&shared, Some(uri.clone()));
    assert_eq!(get_shared_relay_uri(&shared).as_deref(), Some(uri.as_str()));

    set_shared_relay_uri(&shared, None);
    assert_eq!(get_shared_relay_uri(&shared), None);
}

#[test]
fn relay_addr_ipv6_filtering_respects_ipv4_only_flag() {
    let v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1)), 22067);
    let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 22067);

    // PS Vita mode (ipv4_only = true)
    assert!(is_supported_relay_addr(v4, true));
    assert!(!is_supported_relay_addr(v6, true));

    // Host mode (ipv4_only = false)
    assert!(is_supported_relay_addr(v4, false));
    assert!(is_supported_relay_addr(v6, false));
}
