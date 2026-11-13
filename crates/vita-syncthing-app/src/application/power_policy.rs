//! Power management policies: automatic standby and CPU clock.
//!
//! Standby suspends Wi-Fi and drops every BEP session. That is harmless while idle (peers just
//! show the Vita as disconnected and it reconnects on wake-up), but it aborts transfers in
//! progress. Blocking standby costs battery, so it is only done when it is useful.
//!
//! The CPU runs at full speed only during the initial sync phase and transfers; the UI renders frames on demand
//! (`frame_pacing`), so idle screens and waiting sessions run at a lower clock.

use std::time::Duration;

pub use crate::domain::defaults::{CPU_INTERACTIVE_GRACE, CPU_TRANSFER_GRACE, TRANSFER_GRACE};

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

/// ARM CPU clock level of the PS Vita.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuClock {
    /// 111 MHz: static screen, idle or waiting sessions, OLED screensaver.
    Low,
    /// 222 MHz: user interaction (frames rendered on input) and QR code decoding.
    Medium,
    /// 333 MHz (system default): initial sync phase (index exchange, local scan and hashing)
    /// and transfers (TLS decryption, SHA-256 block hashing, disk I/O).
    Full,
}

impl CpuClock {
    /// Frequency passed to `scePowerSetArmClockFrequency`.
    pub fn mhz(self) -> u32 {
        match self {
            CpuClock::Low => 111,
            CpuClock::Medium => 222,
            CpuClock::Full => 333,
        }
    }
}

/// Chooses the CPU clock: full speed only during the initial sync phase and transfers,
/// lower otherwise.
///
/// - `initial_sync`: a peer session is in its initial phase (see `domain::InitialSyncPeers`).
/// - `since_last_transfer`: time since the last download / upload / applied change, if any.
/// - `since_last_input`: time since the last button press, touch or keyboard input.
/// - `camera_active`: the QR scanner is decoding camera frames.
pub fn cpu_clock(
    initial_sync: bool,
    since_last_transfer: Option<Duration>,
    since_last_input: Duration,
    camera_active: bool,
) -> CpuClock {
    if initial_sync || since_last_transfer.is_some_and(|d| d <= CPU_TRANSFER_GRACE) {
        CpuClock::Full
    } else if camera_active || since_last_input <= CPU_INTERACTIVE_GRACE {
        CpuClock::Medium
    } else {
        CpuClock::Low
    }
}
