//! Standby-blocking policy during synchronization.

use std::time::Duration;

use vita_syncthing_app::application::power_policy::should_block_standby;

#[test]
fn test_standby_policy() {
    let recent = Some(Duration::from_secs(10));
    let old = Some(Duration::from_secs(600));
    // No sync running: never block.
    assert!(!should_block_standby(false, true, recent));
    // Charging: always block while syncing.
    assert!(should_block_standby(true, true, None));
    // On battery: only during transfers and the grace period after them.
    assert!(should_block_standby(true, false, recent));
    assert!(!should_block_standby(true, false, old));
    assert!(!should_block_standby(true, false, None));
}

#[test]
fn test_cpu_clock_full_speed_only_while_transferring() {
    use vita_syncthing_app::application::power_policy::{
        cpu_clock, CpuClock, CPU_INTERACTIVE_GRACE, CPU_TRANSFER_GRACE,
    };
    let idle_input = CPU_INTERACTIVE_GRACE + Duration::from_secs(1);
    let recent_input = Duration::from_millis(500);
    let recent_transfer = Some(Duration::from_secs(2));
    let old_transfer = Some(CPU_TRANSFER_GRACE + Duration::from_secs(1));

    // Transfers (and the short grace period after them) run at full speed.
    assert_eq!(
        cpu_clock(recent_transfer, idle_input, false),
        CpuClock::Full
    );
    assert_eq!(
        cpu_clock(recent_transfer, recent_input, true),
        CpuClock::Full
    );
    // User interaction and QR decoding: medium speed.
    assert_eq!(
        cpu_clock(old_transfer, recent_input, false),
        CpuClock::Medium
    );
    assert_eq!(cpu_clock(None, idle_input, true), CpuClock::Medium);
    // Static screen, waiting or idle session: low speed.
    assert_eq!(cpu_clock(old_transfer, idle_input, false), CpuClock::Low);
    assert_eq!(cpu_clock(None, idle_input, false), CpuClock::Low);

    assert_eq!(CpuClock::Low.mhz(), 111);
    assert_eq!(CpuClock::Medium.mhz(), 222);
    assert_eq!(CpuClock::Full.mhz(), 333);
}
