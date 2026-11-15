//! Bounded daemon state fed by unauthenticated input: the "reported once" memory and the
//! listener's slots for connections not authenticated yet.

use syncthing_cli::daemon::listener::ConnectionSlots;
use syncthing_cli::daemon::reports::{ReportMemory, UnknownReport};
use syncthing_cli::daemon::timing::{MAX_REPORTS_PER_SUBJECT, MAX_UNKNOWN_DEVICE_REPORTS};
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
