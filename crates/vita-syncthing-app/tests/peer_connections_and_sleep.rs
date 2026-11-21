//! Direct checks of session claim tracking (`SessionClaims`) and how a worker sleeps while
//! honouring stop requests and wake-ups (`sleep_unless_stopped`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use syncthing_core::{DeviceId, SyncControl};
use vita_syncthing_app::infrastructure::peer_endpoints::sleep_unless_stopped;
use vita_syncthing_app::infrastructure::session_guard::SessionClaims;

#[test]
fn test_session_claims_adds_once_and_releases_only_that_device() {
    let a = DeviceId([0x01; 32]);
    let b = DeviceId([0x02; 32]);
    let claims = Arc::new(SessionClaims::new(4));

    let claim_a = claims.try_claim(a).expect("first claim succeeds");
    assert!(claims.try_claim(a).is_none(), "a device is claimed once");
    assert_eq!(claims.claimed_ids(), vec![a]);

    let _claim_b = claims.try_claim(b).expect("second device claim succeeds");
    assert_eq!(claims.claimed_ids(), vec![a, b]);

    drop(claim_a);
    assert_eq!(
        claims.claimed_ids(),
        vec![b],
        "only the dropped device leaves"
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
        Duration::from_millis(300),
        || false
    ));
    let slept = start.elapsed();
    // Steps of at most 200 ms: 200 + 100 ms, never a longer or shorter total.
    assert!(slept >= Duration::from_millis(290), "slept {slept:?}");
    assert!(slept < Duration::from_millis(360), "slept {slept:?}");
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
