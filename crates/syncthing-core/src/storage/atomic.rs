//! Crash-safe file replacement for PS Vita storage.
//!
//! `sceIoRename` may refuse to overwrite an existing file, and deleting the old file before
//! renaming the new one leaves a window where a crash (or closing the app) loses the file
//! entirely. Here the old file is kept as a backup until the new one is in place, and
//! [`recover_replaced_file`] puts it back if the replacement was interrupted.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to a fresh file at `path` and flushes it to the storage device.
/// (`sceIo` does not reliably honor `O_TRUNC`, so any previous file is removed first.)
pub fn write_file_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let _ = fs::remove_file(path);
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    // Not every filesystem driver implements `fsync`: best effort.
    let _ = file.sync_all();
    Ok(())
}

/// Moves the complete file `tmp` over `dest`. At every instant either `dest` or `backup`
/// holds a complete version, so an interruption never loses the file.
pub fn replace_file(tmp: &Path, dest: &Path, backup: &Path) -> io::Result<()> {
    // Atomic where rename overwrites (POSIX); also the plain case of a new file.
    let Err(direct_err) = fs::rename(tmp, dest) else {
        return Ok(());
    };
    let _ = fs::remove_file(backup);
    if fs::rename(dest, backup).is_err() {
        return Err(direct_err);
    }
    match fs::rename(tmp, dest) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(e) => {
            let _ = fs::rename(backup, dest);
            Err(e)
        }
    }
}

/// Finishes or undoes a [`replace_file`] that was interrupted: if `dest` is missing its
/// backup is restored, otherwise a leftover backup is removed.
/// Returns `true` if `dest` was restored from `backup`.
pub fn recover_replaced_file(dest: &Path, backup: &Path) -> bool {
    if File::open(dest).is_ok() {
        let _ = fs::remove_file(backup);
        return false;
    }
    fs::rename(backup, dest).is_ok()
}
