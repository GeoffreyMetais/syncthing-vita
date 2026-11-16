//! Local Discovery socket: several programs on one computer share UDP 21027.

use syncthing_cli::daemon::lan::{our_announce, LanBeacon};
use syncthing_core::{DeviceIdentity, DISCOVERY_PORT};

#[test]
fn two_beacons_share_the_discovery_port() {
    let id = |seed| DeviceIdentity::from_seed(&[seed; 32]).unwrap().device_id;
    let first = LanBeacon::bind(our_announce(id(71), Some(22000))).unwrap();
    if first.local_port().unwrap() != DISCOVERY_PORT {
        eprintln!("UDP {DISCOVERY_PORT} is held exclusively by another program: not checked");
        return;
    }
    let second = LanBeacon::bind(our_announce(id(72), None)).unwrap();
    assert_eq!(second.local_port().unwrap(), DISCOVERY_PORT);
}

#[test]
fn replies_are_bounded_and_configured_devices_are_always_answered() {
    use std::time::{Duration, Instant};

    use syncthing_cli::daemon::lan::ReplySchedule;
    use syncthing_cli::daemon::timing::{
        LAN_ANNOUNCE_INTERVAL, LAN_REPLY_INTERVAL, MAX_LAN_REPLIES_TO_UNKNOWN,
    };
    use syncthing_core::DeviceId;

    let device = |n: u32| {
        let mut raw = [0u8; 32];
        raw[..4].copy_from_slice(&n.to_le_bytes());
        DeviceId(raw)
    };
    let configured = DeviceIdentity::from_seed(&[73; 32]).unwrap().device_id;
    let mut schedule = ReplySchedule::new(vec![configured]);
    let now = Instant::now();

    for n in 0..MAX_LAN_REPLIES_TO_UNKNOWN as u32 {
        assert!(schedule.is_due(device(n), now));
    }
    // The same device is answered once per interval; other unknown devices wait.
    assert!(!schedule.is_due(device(0), now));
    for n in 0..1000 {
        assert!(!schedule.is_due(device(100_000 + n), now));
    }
    assert_eq!(schedule.len(), MAX_LAN_REPLIES_TO_UNKNOWN);
    // A configured device is answered whatever the unknown ones did.
    assert!(schedule.is_due(configured, now));
    assert!(!schedule.is_due(configured, now));

    // Once the interval elapsed, the old answers are forgotten.
    let later = now + LAN_ANNOUNCE_INTERVAL.max(LAN_REPLY_INTERVAL) + Duration::from_millis(1);
    assert!(schedule.is_due(device(100_000), later));
    assert!(schedule.is_due(device(0), later));
    assert_eq!(schedule.len(), 2);
}

#[test]
fn reply_schedule_is_empty_until_an_answer_is_recorded() {
    use std::time::Instant;

    use syncthing_cli::daemon::lan::ReplySchedule;
    let id = DeviceIdentity::from_seed(&[61; 32])
        .expect("identity")
        .device_id;
    let mut schedule = ReplySchedule::new(vec![id]);
    assert!(schedule.is_empty());
    assert_eq!(schedule.len(), 0);
    assert!(schedule.is_due(id, Instant::now()));
    assert!(!schedule.is_empty());
    assert_eq!(schedule.len(), 1);
}

#[test]
fn reply_is_due_again_exactly_one_interval_after_the_last_answer() {
    use std::time::{Duration, Instant};

    use syncthing_cli::daemon::lan::ReplySchedule;
    use syncthing_cli::daemon::timing::{LAN_ANNOUNCE_INTERVAL, LAN_REPLY_INTERVAL};

    let id = DeviceIdentity::from_seed(&[62; 32])
        .expect("identity")
        .device_id;
    let interval = LAN_ANNOUNCE_INTERVAL.max(LAN_REPLY_INTERVAL);
    let mut schedule = ReplySchedule::new(vec![id]);
    let t0 = Instant::now();
    assert!(schedule.is_due(id, t0));
    assert!(!schedule.is_due(id, t0 + interval - Duration::from_millis(1)));
    assert!(schedule.is_due(id, t0 + interval));
}
