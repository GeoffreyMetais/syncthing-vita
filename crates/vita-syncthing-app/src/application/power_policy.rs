//! When to keep the PS Vita out of automatic standby.
//!
//! Standby suspends Wi-Fi and drops every BEP session. That is harmless while idle (peers just
//! show the Vita as disconnected and it reconnects on wake-up), but it aborts transfers in
//! progress. Blocking standby costs battery, so it is only done when it is useful.

use std::time::Duration;

/// How long standby stays blocked after the last transfer activity.
pub const TRANSFER_GRACE: Duration = Duration::from_secs(120);

/// Returns `true` if automatic standby should be blocked right now.
///
/// - `sync_active`: a sync worker is running or allowed to reconnect.
/// - `charging`: the console is on external power (no battery cost: stay reachable).
/// - `since_last_transfer`: time since the last download / upload / applied change, if any.
pub fn should_block_standby(
    sync_active: bool,
    charging: bool,
    since_last_transfer: Option<Duration>,
) -> bool {
    if !sync_active {
        return false;
    }
    charging || since_last_transfer.is_some_and(|d| d <= TRANSFER_GRACE)
}
