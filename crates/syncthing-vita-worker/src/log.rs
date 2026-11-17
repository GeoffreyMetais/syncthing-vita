//! The worker's lines in the app's protocol log (`syncthing.log`), with the app's prefix and
//! rotation: above `LOG_MAX_BYTES` the file becomes `syncthing.log.1` and a new one starts.
//! The app and the worker never run at the same time, so they never append concurrently.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::defaults::{LOG_LINE_PREFIX, LOG_MAX_BYTES, LOG_TAG};

/// Name of the log file in the app's data directory.
pub const LOG_FILE: &str = "syncthing.log";
/// Name of the previous log file, kept by the rotation.
pub const ROTATED_LOG_FILE: &str = "syncthing.log.1";

/// Appends lines to `<data_dir>/syncthing.log`.
#[derive(Debug, Clone)]
pub struct WorkerLog {
    path: PathBuf,
    rotated: PathBuf,
    max_bytes: u64,
}

impl WorkerLog {
    pub fn new(data_dir: &Path) -> Self {
        Self::with_limit(data_dir, LOG_MAX_BYTES)
    }

    /// A log rotated above `max_bytes` (tests use small limits).
    pub fn with_limit(data_dir: &Path, max_bytes: u64) -> Self {
        Self {
            path: data_dir.join(LOG_FILE),
            rotated: data_dir.join(ROTATED_LOG_FILE),
            max_bytes,
        }
    }

    /// Appends `msg` as one line (`[VITA-SYNCTHING] Background sync: <msg>`). Errors are
    /// ignored: logging never stops a cycle. Peer-supplied text is written without control
    /// characters, so it cannot forge lines.
    pub fn line(&self, msg: &str) {
        let printable: String = msg
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = writeln!(file, "{LOG_LINE_PREFIX}{LOG_TAG}{printable}");
        let too_large = file.metadata().is_ok_and(|m| m.len() > self.max_bytes);
        drop(file);
        if too_large {
            let _ = fs::remove_file(&self.rotated);
            let _ = fs::rename(&self.path, &self.rotated);
        }
    }
}
