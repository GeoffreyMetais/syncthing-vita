//! Folder reconciliation on top of the persistent local database ([`crate::local_db`]).
//!
//! - [`scan`] — local disk scans and local `Index` generation
//! - [`pull`] — pull planning against a peer's index (deletions, directories, conflicts)
//!   and bookkeeping of completed downloads

mod pull;
mod scan;

use std::fs;
use std::time::UNIX_EPOCH;

use crate::bep::BepFileInfo;

/// What must happen locally to catch up with a peer's index.
#[derive(Debug, Default)]
pub struct PullPlan {
    /// Files whose newer remote version must be downloaded.
    pub downloads: Vec<BepFileInfo>,
    /// Remote files the Vita already has (same content).
    pub up_to_date: Vec<BepFileInfo>,
    /// Human-readable list of directory creations, deletions and conflict copies applied.
    pub applied: Vec<String>,
}

fn mtime_secs(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
