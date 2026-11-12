//! Tunable timings of a persistent BEP session, chosen by the application.

use std::time::Duration;

/// Timings of a persistent session. The defaults suit a handheld on Wi-Fi and battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncTuning {
    /// Interval between automatic rescans of the local folders. A rescan walks every shared
    /// folder, so short intervals cost CPU, storage I/O and battery. A manual rescan
    /// ([`super::SyncControl::request_rescan`]) is always immediate.
    pub rescan_interval: Duration,
    /// Maximum silence on a connection before a `Ping` is sent, including during long local
    /// scans. Syncthing drops a connection after 5 minutes without any message.
    pub keepalive_interval: Duration,
}

impl Default for SyncTuning {
    fn default() -> Self {
        Self {
            rescan_interval: Duration::from_secs(60),
            keepalive_interval: Duration::from_secs(20),
        }
    }
}
