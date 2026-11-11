//! Thread-safe control handle shared between the UI thread and a persistent BEP session.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Thread-safe control handle allowing the UI thread to request an immediate local folder rescan
/// or a clean shutdown of a persistent BEP/1.0 session.
#[derive(Debug, Default)]
pub struct SyncControl {
    stop_requested: AtomicBool,
    rescan_requested: AtomicBool,
    rescan_epoch: AtomicU64,
}

impl SyncControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
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
}
