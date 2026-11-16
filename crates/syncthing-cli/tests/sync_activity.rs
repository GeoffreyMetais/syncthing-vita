//! `run --exit-when-synced`: a sync is complete once every required device finished a cycle
//! and no transfer happened for the quiet period.

use std::time::Duration;

use syncthing_cli::daemon::activity::SyncActivity;
use syncthing_core::DeviceIdentity;

fn device(seed: u8) -> syncthing_core::DeviceId {
    DeviceIdentity::from_seed(&[seed; 32])
        .expect("identity")
        .device_id
}

#[test]
fn nothing_is_synced_without_required_devices() {
    let activity = SyncActivity::default();
    assert!(!activity.is_synced(&[], Duration::ZERO));
}

#[test]
fn every_required_device_must_complete_a_cycle() {
    let (a, b) = (device(50), device(51));
    let mut activity = SyncActivity::default();
    activity.record_cycle(a);
    assert!(activity.is_synced(&[a], Duration::ZERO));
    assert!(!activity.is_synced(&[a, b], Duration::ZERO));
    activity.record_cycle(b);
    assert!(activity.is_synced(&[a, b], Duration::ZERO));
}

#[test]
fn recording_a_cycle_twice_changes_nothing() {
    let a = device(52);
    let mut activity = SyncActivity::default();
    activity.record_cycle(a);
    activity.record_cycle(a);
    assert!(activity.is_synced(&[a], Duration::ZERO));
}

#[test]
fn a_recent_transfer_delays_the_sync_until_the_quiet_period_passed() {
    let a = device(53);
    let mut activity = SyncActivity::default();
    activity.record_cycle(a);
    activity.record_transfer();
    assert!(!activity.is_synced(&[a], Duration::from_secs(3600)));
    assert!(activity.is_synced(&[a], Duration::ZERO));
}

#[test]
fn a_transfer_is_recorded_before_the_sync_is_declared() {
    // Without any transfer, the quiet period does not apply.
    let a = device(54);
    let mut activity = SyncActivity::default();
    activity.record_cycle(a);
    assert!(activity.is_synced(&[a], Duration::from_secs(3600)));
}
