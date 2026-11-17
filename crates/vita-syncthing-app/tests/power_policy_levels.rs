//! Power policy boundaries: standby blocking and CPU clock levels at the exact grace limits.

use std::time::Duration;

use vita_syncthing_app::application::power_policy::{
    cpu_clock, should_block_standby, CpuClock, CPU_INTERACTIVE_GRACE, CPU_TRANSFER_GRACE,
    TRANSFER_GRACE,
};

const ONE_MS: Duration = Duration::from_millis(1);

#[test]
fn clock_frequencies_match_the_three_levels() {
    assert_eq!(CpuClock::Low.mhz(), 111);
    assert_eq!(CpuClock::Medium.mhz(), 222);
    assert_eq!(CpuClock::Full.mhz(), 333);
}

#[test]
fn standby_is_blocked_up_to_exactly_the_transfer_grace() {
    // Only a running sync blocks standby.
    assert!(!should_block_standby(false, true, Some(Duration::ZERO)));

    // On battery the grace period is inclusive at its end, and over after it.
    assert!(should_block_standby(true, false, Some(Duration::ZERO)));
    assert!(should_block_standby(true, false, Some(TRANSFER_GRACE)));
    assert!(!should_block_standby(
        true,
        false,
        Some(TRANSFER_GRACE + ONE_MS)
    ));

    // Charging blocks standby whatever the transfer age, even without a transfer.
    assert!(should_block_standby(
        true,
        true,
        Some(TRANSFER_GRACE + ONE_MS)
    ));
    assert!(should_block_standby(true, true, None));
}

#[test]
fn full_clock_is_used_during_initial_sync_whatever_the_activity() {
    let idle = Duration::from_secs(3600);
    assert_eq!(cpu_clock(true, None, idle, false), CpuClock::Full);
    assert_eq!(cpu_clock(true, None, idle, true), CpuClock::Full);
}

#[test]
fn full_clock_follows_the_transfer_grace_boundary() {
    let idle = CPU_INTERACTIVE_GRACE + Duration::from_secs(60);
    assert_eq!(
        cpu_clock(false, Some(CPU_TRANSFER_GRACE), idle, false),
        CpuClock::Full,
        "the grace limit itself is still full speed"
    );
    assert_eq!(
        cpu_clock(false, Some(CPU_TRANSFER_GRACE + ONE_MS), idle, false),
        CpuClock::Low
    );
    assert_eq!(
        cpu_clock(false, Some(Duration::ZERO), idle, false),
        CpuClock::Full
    );
}

#[test]
fn medium_clock_follows_the_interactive_grace_and_the_camera() {
    // Input within the grace (inclusive) runs at medium speed.
    assert_eq!(
        cpu_clock(false, None, CPU_INTERACTIVE_GRACE, false),
        CpuClock::Medium
    );
    assert_eq!(
        cpu_clock(false, None, CPU_INTERACTIVE_GRACE + ONE_MS, false),
        CpuClock::Low
    );
    assert_eq!(
        cpu_clock(false, None, Duration::ZERO, false),
        CpuClock::Medium
    );

    // The QR scanner keeps the medium clock even without input.
    assert_eq!(
        cpu_clock(false, None, Duration::from_secs(600), true),
        CpuClock::Medium
    );

    // Full speed still wins over the camera and the input.
    assert_eq!(cpu_clock(true, None, Duration::ZERO, true), CpuClock::Full);
    assert_eq!(
        cpu_clock(false, Some(Duration::ZERO), Duration::ZERO, true),
        CpuClock::Full
    );
}

#[test]
fn low_clock_when_nothing_happens() {
    let idle = Duration::from_secs(600);
    assert_eq!(cpu_clock(false, None, idle, false), CpuClock::Low);
    assert_eq!(
        cpu_clock(
            false,
            Some(CPU_TRANSFER_GRACE + Duration::from_secs(1)),
            idle,
            false
        ),
        CpuClock::Low
    );
}
