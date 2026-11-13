//! Thread-safe control handle shared between the UI thread and a persistent BEP session.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::tuning::SyncTuning;

/// Thread-safe control handle allowing the UI thread to request an immediate local folder rescan
/// or a clean shutdown of a persistent BEP/1.0 session.
#[derive(Debug, Default)]
pub struct SyncControl {
    stop_requested: AtomicBool,
    rescan_requested: AtomicBool,
    rescan_epoch: AtomicU64,
    discovery_requested: AtomicBool,
    /// Receive-only folders whose local changes must be reverted, until a session takes them.
    revert_requests: Mutex<Vec<String>>,
    tuning: SyncTuning,
}

impl SyncControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Creates a control handle for sessions running with `tuning`.
    pub fn with_tuning(tuning: SyncTuning) -> Arc<Self> {
        Arc::new(Self {
            tuning,
            ..Self::default()
        })
    }

    /// Timings of the sessions driven by this handle.
    pub fn tuning(&self) -> SyncTuning {
        self.tuning
    }

    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }

    pub fn is_stop_requested(&self) -> bool {
        self.stop_requested.load(Ordering::SeqCst)
    }

    pub fn request_rescan(&self) {
        self.rescan_requested.store(true, Ordering::SeqCst);
        self.rescan_epoch.fetch_add(1, Ordering::SeqCst);
    }

    pub fn take_rescan_request(&self) -> bool {
        self.rescan_requested.swap(false, Ordering::SeqCst)
    }

    pub fn rescan_epoch(&self) -> u64 {
        self.rescan_epoch.load(Ordering::SeqCst)
    }

    /// Asks the application's discovery loop for a LAN discovery cycle now, instead of at its
    /// next periodic one (e.g. the user is looking for a device to add).
    pub fn request_discovery(&self) {
        self.discovery_requested.store(true, Ordering::SeqCst);
    }

    /// Returns `true` once per [`Self::request_discovery`] call (the request is consumed).
    pub fn take_discovery_request(&self) -> bool {
        self.discovery_requested.swap(false, Ordering::SeqCst)
    }

    /// Asks the next session that synchronizes `folder_id` to revert its local changes.
    pub fn request_revert(&self, folder_id: &str) {
        let mut requests = self
            .revert_requests
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if !requests.iter().any(|f| f == folder_id) {
            requests.push(folder_id.to_owned());
        }
    }

    /// Removes and returns the pending revert requests `handles` accepts; the others stay
    /// pending for another session. Each request is handled once.
    pub fn take_revert_requests(&self, handles: impl Fn(&str) -> bool) -> Vec<String> {
        let mut requests = self
            .revert_requests
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (taken, kept): (Vec<String>, Vec<String>) =
            requests.drain(..).partition(|f| handles(f));
        *requests = kept;
        taken
    }
}
