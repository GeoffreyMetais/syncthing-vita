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

/// How a single-cycle session ([`super::SyncControl::single_cycle`]) ends once its index is
/// published: it keeps answering the peer (block `Request`s of our local changes, `Ping`s,
/// late `Index` updates) until the peer has been silent for `idle`, and never longer than
/// `max` in total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleDrain {
    /// Silence of the peer after which the cycle is over.
    pub idle: Duration,
    /// Upper bound of the whole drain, whatever the peer does.
    pub max: Duration,
}

impl CycleDrain {
    /// A drain of exactly `window`, however active the peer is.
    pub fn fixed(window: Duration) -> Self {
        Self {
            idle: window,
            max: window,
        }
    }

    /// Returns `true` once the drain is over: `since_activity` of silence, or `elapsed` in total.
    pub fn is_over(&self, elapsed: Duration, since_activity: Duration) -> bool {
        elapsed >= self.max || since_activity >= self.idle
    }
}
