//! Hand-over from the background sync plugin to the app (`BackgroundHandover`).
//!
//! The plugin's launcher holds `SyncthingWorkerRunning` while a background worker module is
//! loaded. It asks that worker to abort when the app starts, but the worker may need a few
//! seconds (up to a block timeout) to stop. The app's first sync worker must not run at the same
//! time (both would write the same folders and databases), so it waits while the mutex exists,
//! at most `BACKGROUND_WORKER_WAIT`. The main loop keeps running meanwhile; the gate is polled
//! once per iteration and only decides.

use std::time::{Duration, Instant};

use crate::domain::defaults::BACKGROUND_WORKER_WAIT;

/// Outcome of one `BackgroundHandover::check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoverStep {
    /// A background worker still runs: do not start a sync worker yet.
    Wait,
    /// The app may start its sync workers from now on (reported once).
    Released(ReleaseReason),
    /// Released by an earlier check.
    AlreadyReleased,
}

/// Why the hand-over ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseReason {
    /// No background worker was running at the first check.
    NoWorker,
    /// The background worker stopped after waiting `Duration`.
    Stopped(Duration),
    /// The background worker was still running after `BACKGROUND_WORKER_WAIT`.
    TimedOut,
}

/// Waits for a running background worker before the app's first sync worker.
#[derive(Debug, Clone, Default)]
pub struct BackgroundHandover {
    /// When the first check saw a running worker.
    waiting_since: Option<Instant>,
    released: bool,
}

impl BackgroundHandover {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once the app may start sync workers.
    pub fn is_released(&self) -> bool {
        self.released
    }

    /// One check at `now`, given whether a background worker runs (`worker_running`).
    pub fn check(&mut self, worker_running: bool, now: Instant) -> HandoverStep {
        if self.released {
            return HandoverStep::AlreadyReleased;
        }
        if !worker_running {
            self.released = true;
            return HandoverStep::Released(match self.waiting_since {
                Some(since) => ReleaseReason::Stopped(now.saturating_duration_since(since)),
                None => ReleaseReason::NoWorker,
            });
        }
        let since = *self.waiting_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= BACKGROUND_WORKER_WAIT {
            self.released = true;
            return HandoverStep::Released(ReleaseReason::TimedOut);
        }
        HandoverStep::Wait
    }
}
