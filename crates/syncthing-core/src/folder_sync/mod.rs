//! Folder reconciliation on top of the persistent local database ([`crate::local_db`]).
//!
//! - [`scan`] — local disk scans and local `Index` generation
//! - [`ignore`] — `.stignore` pattern parsing and matching
//! - [`pull`] — pull planning against a peer's index (deletions, directories, conflicts)
//!   and bookkeeping of completed downloads
//! - [`receive_only`] — local changes of receive-only folders are published as invalid
//! - [`send_only`] — overriding remote changes of send-only folders

pub mod ignore;
mod pull;
pub mod receive_only;
mod scan;
pub mod send_only;

pub use ignore::IgnoreMatcher;
pub use receive_only::mask_local_changes;

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

pub(crate) fn mtime_parts(meta: &fs::Metadata) -> (i64, i32) {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| (d.as_secs() as i64, d.subsec_nanos() as i32))
        .unwrap_or((0, 0))
}

#[cfg(all(unix, not(target_env = "newlib")))]
pub(crate) fn posix_permissions(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o777
}

#[cfg(not(all(unix, not(target_env = "newlib"))))]
pub(crate) fn posix_permissions(_meta: &fs::Metadata) -> u32 {
    0
}

#[cfg(all(unix, not(target_env = "newlib")))]
pub(crate) fn apply_posix_permissions(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if mode != 0 {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777));
    }
}

#[cfg(not(all(unix, not(target_env = "newlib"))))]
pub(crate) fn apply_posix_permissions(_path: &std::path::Path, _mode: u32) {}
