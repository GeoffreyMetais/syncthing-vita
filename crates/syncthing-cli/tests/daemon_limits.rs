//! Bounded daemon state fed by unauthenticated input: the "reported once" memory, the slots
//! for connections not authenticated yet and the triage of relay invitations.

use syncthing_cli::daemon::connection_slots::ConnectionSlots;
use syncthing_cli::daemon::relay::{triage_invitation, InvitationTriage};
use syncthing_cli::daemon::reports::{ReportMemory, UnknownReport};
use syncthing_cli::daemon::timing::{
    MAX_PENDING_RELAY_INVITATIONS, MAX_REPORTS_PER_SUBJECT, MAX_UNKNOWN_DEVICE_REPORTS,
};
use syncthing_core::{DeviceId, DeviceIdentity};

/// A distinct device ID per `n` (no certificate needed: unknown IDs are arbitrary bytes).
fn device(n: u32) -> DeviceId {
    let mut raw = [0u8; 32];
    raw[..4].copy_from_slice(&n.to_le_bytes());
    DeviceId(raw)
}

#[test]
fn unknown_devices_are_reported_once_each_up_to_the_limit() {
    let mut memory = ReportMemory::default();
    assert_eq!(
        memory.unknown_device("lan", device(0)),
        UnknownReport::First
    );
    assert_eq!(
        memory.unknown_device("lan", device(0)),
        UnknownReport::Quiet
    );
    // Another channel is another report.
    assert_eq!(
        memory.unknown_device("tcp", device(0)),
        UnknownReport::First
    );

    for n in 1..(MAX_UNKNOWN_DEVICE_REPORTS as u32 - 1) {
        assert_eq!(
            memory.unknown_device("lan", device(n)),
            UnknownReport::First
        );
    }
    assert_eq!(memory.len(), MAX_UNKNOWN_DEVICE_REPORTS);
    // Beyond the limit: one line saying so, then silence, and no growth.
    let flood = MAX_UNKNOWN_DEVICE_REPORTS as u32 + 10_000;
    assert_eq!(
        memory.unknown_device("lan", device(flood)),
        UnknownReport::LimitReached
    );
    for n in flood + 1..flood + 1000 {
        assert_eq!(
            memory.unknown_device("relay", device(n)),
            UnknownReport::Quiet
        );
    }
    assert_eq!(memory.len(), MAX_UNKNOWN_DEVICE_REPORTS);
    // Devices reported before the limit stay quiet.
    assert_eq!(
        memory.unknown_device("lan", device(1)),
        UnknownReport::Quiet
    );
}

#[test]
fn reports_about_configured_devices_survive_a_flood_of_unknown_devices() {
    let known = DeviceIdentity::from_seed(&[81u8; 32]).unwrap().device_id;
    let mut memory = ReportMemory::default();
    for n in 0..(MAX_UNKNOWN_DEVICE_REPORTS as u32 * 4) {
        memory.unknown_device("lan", device(n));
    }
    let subject = format!("in-sync:{known}");
    assert!(memory.first(&subject, ""));
    assert!(!memory.first(&subject, ""));
    assert!(memory.first(&format!("offer:{known}"), "photos"));
    assert!(memory.first("announced:discovery.example", "[\"tcp://0.0.0.0:22000\"]"));
}

#[test]
fn a_subject_keeps_a_bounded_number_of_details() {
    let known = DeviceIdentity::from_seed(&[82u8; 32]).unwrap().device_id;
    let lan = format!("lan:{known}");
    let mut memory = ReportMemory::default();
    // A spoofed announcement for a configured device with ever-changing addresses.
    for port in 0..MAX_REPORTS_PER_SUBJECT as u16 {
        assert!(memory.first(&lan, &format!("[10.0.0.1:{port}]")));
    }
    for port in 0..1000u16 {
        assert!(!memory.first(&lan, &format!("[10.0.0.2:{port}]")));
    }
    assert_eq!(memory.len(), 1 + MAX_REPORTS_PER_SUBJECT);
    // The other subjects of that device are not crowded out.
    assert!(memory.first(&format!("in-sync:{known}"), ""));
}

#[test]
fn connection_slots_are_bounded_and_given_back_when_dropped() {
    let slots = ConnectionSlots::new(2);
    let first = slots.try_take().expect("first slot");
    let second = slots.try_take().expect("second slot");
    assert!(slots.try_take().is_none(), "all slots are taken");
    assert_eq!(slots.in_use(), 2);
    drop(first);
    assert_eq!(slots.in_use(), 1);
    let third = slots.try_take().expect("a slot was given back");
    // A slot moved into a thread that ends (or panics) is given back too.
    std::thread::spawn(move || {
        let _held = third;
        panic!("connection handler failed");
    })
    .join()
    .unwrap_err();
    drop(second);
    assert_eq!(slots.in_use(), 0);
}

/// Triages an invitation from `from` for a daemon configured with `configured`, of which
/// `connected` already have a session.
fn triage(
    from: &DeviceId,
    configured: &[DeviceId],
    connected: &[DeviceId],
    slots: &ConnectionSlots,
) -> InvitationTriage {
    triage_invitation(
        &from.0,
        |id| configured.contains(id),
        |id| connected.contains(id),
        slots,
    )
}

#[test]
fn relay_invitations_needing_no_session_take_no_slot() {
    let slots = ConnectionSlots::new(MAX_PENDING_RELAY_INVITATIONS);
    let (known, connected) = (device(1), device(2));
    let configured = [known, connected];
    assert!(matches!(
        triage_invitation(&[7u8; 31], |_| true, |_| false, &slots),
        InvitationTriage::Malformed
    ));
    assert!(matches!(
        triage(&device(3), &configured, &[connected], &slots),
        InvitationTriage::UnknownDevice(id) if id == device(3)
    ));
    assert!(matches!(
        triage(&connected, &configured, &[connected], &slots),
        InvitationTriage::AlreadyConnected(id) if id == connected
    ));
    assert_eq!(slots.in_use(), 0);
    match triage(&known, &configured, &[connected], &slots) {
        InvitationTriage::Join(id, _slot) => {
            assert_eq!(id, known);
            assert_eq!(slots.in_use(), 1);
        }
        other => panic!("expected a join, got {other:?}"),
    }
    assert_eq!(slots.in_use(), 0, "the slot is given back with the join");
}

#[test]
fn relay_invitations_beyond_the_limit_are_dropped_until_slots_come_back() {
    let slots = ConnectionSlots::new(MAX_PENDING_RELAY_INVITATIONS);
    let known = device(1);
    let mut pending = Vec::new();
    for _ in 0..MAX_PENDING_RELAY_INVITATIONS {
        match triage(&known, &[known], &[], &slots) {
            InvitationTriage::Join(_, slot) => pending.push(slot),
            other => panic!("expected a join, got {other:?}"),
        }
    }
    for _ in 0..100 {
        assert!(matches!(
            triage(&known, &[known], &[], &slots),
            InvitationTriage::Busy(id) if id == known
        ));
    }
    assert_eq!(slots.in_use(), MAX_PENDING_RELAY_INVITATIONS);
    // A pending invitation becoming a session (or failing) gives its slot back.
    pending.pop();
    assert!(matches!(
        triage(&known, &[known], &[], &slots),
        InvitationTriage::Join(..)
    ));
    pending.clear();
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn report_memory_is_empty_until_something_is_reported() {
    let mut memory = ReportMemory::default();
    assert!(memory.is_empty());
    assert_eq!(memory.len(), 0);
    assert!(memory.first("lookup:x", "timeout"));
    assert!(!memory.is_empty());
    assert_eq!(memory.len(), 2, "one subject and one detail");
}

#[test]
fn report_memory_counts_unknown_devices_too() {
    let mut memory = ReportMemory::default();
    assert_eq!(
        memory.unknown_device("lan", device(7)),
        UnknownReport::First
    );
    assert!(!memory.is_empty());
    assert_eq!(memory.len(), 1);
}
