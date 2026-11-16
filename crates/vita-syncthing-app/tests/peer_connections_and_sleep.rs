//! Direct checks of two helpers of `peer_endpoints` that decide which configured devices are
//! connected (`set_connected`) and how a worker sleeps while honouring stop requests and wake-ups
//! (`sleep_unless_stopped`). Both are reached through the public module only.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use syncthing_core::{DeviceId, SyncControl};
use vita_syncthing_app::infrastructure::peer_endpoints::{set_connected, sleep_unless_stopped};

fn connected_list(ids: &[DeviceId]) -> Arc<Mutex<Vec<DeviceId>>> {
    Arc::new(Mutex::new(ids.to_vec()))
}

#[test]
fn test_set_connected_adds_once_and_removes_only_that_device() {
    let a = DeviceId([0x01; 32]);
    let b = DeviceId([0x02; 32]);
    let list = connected_list(&[]);

    set_connected(&list, a, true);
    set_connected(&list, a, true);
    assert_eq!(*list.lock().unwrap(), vec![a], "a device is listed once");

    set_connected(&list, b, true);
    assert_eq!(*list.lock().unwrap(), vec![a, b]);

    set_connected(&list, a, false);
    assert_eq!(
        *list.lock().unwrap(),
        vec![b],
        "only the named device leaves"
    );

    set_connected(&list, a, false);
    assert_eq!(
        *list.lock().unwrap(),
        vec![b],
        "removing an absent device changes nothing"
    );
}

#[test]
fn test_sleep_returns_false_at_once_when_stop_is_requested() {
    let control = SyncControl::new();
    control.request_stop();
    let start = Instant::now();
    assert!(!sleep_unless_stopped(
        &control,
        Duration::from_secs(10),
        || false
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn test_sleep_returns_true_at_once_when_woken() {
    let control = SyncControl::new();
    let start = Instant::now();
    assert!(sleep_unless_stopped(
        &control,
        Duration::from_secs(10),
        || true
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn test_sleep_runs_its_full_duration_then_reports_to_carry_on() {
    let control = SyncControl::new();
    let start = Instant::now();
    assert!(sleep_unless_stopped(
        &control,
        Duration::from_millis(450),
        || false
    ));
    let slept = start.elapsed();
    assert!(slept >= Duration::from_millis(440), "slept {slept:?}");
    assert!(slept < Duration::from_secs(3), "slept {slept:?}");
}

#[test]
fn test_zero_duration_returns_immediately_with_carry_on() {
    let control = SyncControl::new();
    assert!(sleep_unless_stopped(&control, Duration::ZERO, || false));
}

#[test]
fn test_interruptible_sleep_gives_up_on_stop_and_carries_on_otherwise() {
    use vita_syncthing_app::infrastructure::peer_endpoints::sleep_interruptible;
    let control = SyncControl::new();
    control.request_stop();
    let start = Instant::now();
    assert!(!sleep_interruptible(&control, Duration::from_secs(10)));
    assert!(start.elapsed() < Duration::from_secs(2));

    let running = SyncControl::new();
    assert!(sleep_interruptible(&running, Duration::from_millis(20)));
}

#[test]
fn test_endpoint_labels_show_direct_and_relay_addresses() {
    use std::net::SocketAddr;
    use syncthing_core::PeerEndpoint;
    use vita_syncthing_app::infrastructure::peer_endpoints::format_endpoint_label;
    let direct: SocketAddr = "10.0.0.8:22000".parse().unwrap();
    let relay: SocketAddr = "198.51.100.9:22067".parse().unwrap();
    assert_eq!(
        format_endpoint_label(&PeerEndpoint::Direct(direct)),
        "10.0.0.8:22000"
    );
    assert_eq!(
        format_endpoint_label(&PeerEndpoint::Relay {
            addr: relay,
            device_id: DeviceId([0x09; 32]),
        }),
        "relay://198.51.100.9:22067"
    );
}
