//! Crash-safe download assembly: blocks are streamed into a hidden temp file which is only
//! moved into place once complete, so a half-written file is never indexed or served.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::PathBuf;

use super::vita_fs::ensure_dir_exists;
use super::FsSyncStorage;

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
    pub fn commit_download(
        &self,
        folder_id: &str,
        rel_path: &str,
        final_size: u64,
    ) -> io::Result<()> {
        let full = self.resolve_safe_path(folder_id, rel_path);
        let tmp = self.download_temp_path(folder_id, rel_path);
        OpenOptions::new()
            .write(true)
            .open(&tmp)?
            .set_len(final_size)?;
        if full.exists() {
            fs::remove_file(&full).map_err(|e| {
                io::Error::new(e.kind(), format!("replace {}: {e}", full.display()))
            })?;
        }
        fs::rename(&tmp, &full)
            .map_err(|e| io::Error::new(e.kind(), format!("rename {}: {e}", full.display())))
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
