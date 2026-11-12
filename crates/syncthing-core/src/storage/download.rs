//! Crash-safe download assembly: blocks are streamed into a hidden temp file which is only
//! moved into place once complete, so a half-written file is never indexed or served.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::atomic::{recover_replaced_file, replace_file};
use super::vita_fs::ensure_dir_exists;
use super::FsSyncStorage;

const BACKUP_PREFIX: &str = ".syncthing.";
const BACKUP_SUFFIX: &str = ".bak";

/// Returns `true` if `name` is a backup file written by [`FsSyncStorage::commit_download`].
pub(crate) fn is_download_backup_name(name: &str) -> bool {
    name.len() > BACKUP_PREFIX.len() + BACKUP_SUFFIX.len()
        && name.starts_with(BACKUP_PREFIX)
        && name.ends_with(BACKUP_SUFFIX)
}

/// Finishes or undoes download commits that were interrupted (crash, app closed): a file that
/// is missing while its backup exists is restored, a leftover backup is removed.
/// Returns `true` if at least one file was restored.
pub(crate) fn recover_interrupted_commits(backups: &[PathBuf]) -> bool {
    let mut restored = false;
    for backup in backups {
        let Some(name) = backup.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_download_backup_name(name) {
            continue;
        }
        let original = &name[BACKUP_PREFIX.len()..name.len() - BACKUP_SUFFIX.len()];
        let dest: PathBuf = backup.with_file_name(original);
        restored |= recover_replaced_file(Path::new(&dest), backup);
    }
    restored
}

impl FsSyncStorage {
    /// Hidden temporary file a download is assembled into before being moved into place, so a
    /// half-written file is never indexed or served as a local change.
    pub fn download_temp_path(&self, folder_id: &str, rel_path: &str) -> PathBuf {
        let full = self.resolve_safe_path(folder_id, rel_path);
        let file_name = full
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        full.with_file_name(format!(".syncthing.{}.tmp", file_name))
    }

    /// Hidden backup of the previous version while a finished download is moved into place.
    pub fn download_backup_path(&self, folder_id: &str, rel_path: &str) -> PathBuf {
        let full = self.resolve_safe_path(folder_id, rel_path);
        let file_name = full
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        full.with_file_name(format!("{BACKUP_PREFIX}{file_name}{BACKUP_SUFFIX}"))
    }

    /// Starts a download: seeds the temp file with the current local content (delta sync only
    /// rewrites outdated blocks).
    pub fn prepare_download(&self, folder_id: &str, rel_path: &str) -> io::Result<()> {
        let full = self.resolve_safe_path(folder_id, rel_path);
        if let Some(parent) = full.parent() {
            ensure_dir_exists(parent)?;
        }
        let tmp = self.download_temp_path(folder_id, rel_path);
        let _ = fs::remove_file(&tmp);
        if full.is_file() {
            fs::copy(&full, &tmp)
                .map_err(|e| io::Error::new(e.kind(), format!("copy {}: {e}", full.display())))?;
        } else {
            File::create(&tmp)
                .map_err(|e| io::Error::new(e.kind(), format!("create {}: {e}", tmp.display())))?;
        }
        Ok(())
    }

    /// Finishes a download: truncates the temp file to `final_size` and moves it into place.
    /// The previous version is kept as a backup until the new one is in place, so an
    /// interruption never leaves the file missing (see [`recover_interrupted_commits`]).
    pub fn commit_download(
        &self,
        folder_id: &str,
        rel_path: &str,
        final_size: u64,
    ) -> io::Result<()> {
        let full = self.resolve_safe_path(folder_id, rel_path);
        let tmp = self.download_temp_path(folder_id, rel_path);
        let file = OpenOptions::new().write(true).open(&tmp)?;
        file.set_len(final_size)?;
        // Not every filesystem driver implements `fsync`: best effort.
        let _ = file.sync_all();
        drop(file);
        let backup = self.download_backup_path(folder_id, rel_path);
        replace_file(&tmp, &full, &backup)
            .map_err(|e| io::Error::new(e.kind(), format!("replace {}: {e}", full.display())))
    }

    /// Discards an unfinished download.
    pub fn abort_download(&self, folder_id: &str, rel_path: &str) {
        let _ = fs::remove_file(self.download_temp_path(folder_id, rel_path));
    }

    /// **Step 1 (Streaming Disk Write)**:
    /// Writes a downloaded block at its `offset` into the download temp file (see
    /// [`Self::prepare_download`]) without buffering the whole file in RAM.
    pub fn write_block_streaming(
        &self,
        folder_id: &str,
        rel_path: &str,
        offset: u64,
        data: &[u8],
        final_file_size: u64,
    ) -> io::Result<PathBuf> {
        let full_path = self.download_temp_path(folder_id, rel_path);
        if let Some(parent) = full_path.parent() {
            ensure_dir_exists(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&full_path)
            .map_err(|e| io::Error::new(e.kind(), format!("open {}: {e}", full_path.display())))?;

        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;

        let current_len = file.metadata()?.len();
        if current_len > final_file_size {
            file.set_len(final_file_size)?;
        }

        Ok(full_path)
    }
}
