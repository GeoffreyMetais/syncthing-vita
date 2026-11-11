//! Shared helpers for `syncthing-core` integration tests.

use std::path::PathBuf;

/// Returns an empty, test-specific directory under the system temp dir.
pub fn fresh_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vita_syncthing_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
