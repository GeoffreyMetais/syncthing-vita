//! Sync activity, for `run --exit-when-synced`: which devices completed a sync cycle, and
//! when data last moved.

use std::time::{Duration, Instant};

use syncthing_core::DeviceId;

#[derive(Debug, Default)]
pub struct SyncActivity {
    /// Devices that completed at least one sync cycle.
    in_sync: Vec<DeviceId>,
    /// Last cycle completion or block/local change received/sent.
    last_activity: Option<Instant>,
}

impl SyncActivity {
    pub fn record_cycle(&mut self, device: DeviceId) {
        if !self.in_sync.contains(&device) {
            self.in_sync.push(device);
        }
        self.last_activity = Some(Instant::now());
    }

    pub fn record_transfer(&mut self) {
        self.last_activity = Some(Instant::now());
    }

    /// `true` once every device of `required` completed a sync cycle and no cycle or transfer
    /// happened for `quiet` (a peer may still be finishing its initial cycle or pulling from us).
    pub fn is_synced(&self, required: &[DeviceId], quiet: Duration) -> bool {
        !required.is_empty()
            && required.iter().all(|d| self.in_sync.contains(d))
            && self.last_activity.is_some_and(|t| t.elapsed() >= quiet)
    }
}
