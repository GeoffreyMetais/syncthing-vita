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
