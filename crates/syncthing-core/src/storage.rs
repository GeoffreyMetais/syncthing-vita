//! 128 KiB block streaming storage, Delta Sync verification,
//! custom folder routing (`ux0:...`), and bidirectional local indexing.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::bep::{
    BepBlockInfo, BepFileInfo, BepFolder, BEP_BLOCK_SIZE,
};

/// Local filesystem storage manager (`ux0:data/syncthing/sync` or custom paths).
#[derive(Debug, Clone)]
pub struct FsSyncStorage {
    default_root_dir: PathBuf,
    /// Custom target paths per `folder_id` (e.g. `"xkx75-ukkm3" -> "ux0:music/Test sv"`)
    custom_folder_paths: HashMap<String, PathBuf>,
    /// Folders paused by user
    paused_folders: HashSet<String>,
    /// Locally configured folders to advertise in `ClusterConfig`
    local_folders: Vec<BepFolder>,
}

impl FsSyncStorage {
    pub fn new<P: Into<PathBuf>>(default_root_dir: P) -> Self {
        Self {
            default_root_dir: default_root_dir.into(),
            custom_folder_paths: HashMap::new(),
            paused_folders: HashSet::new(),
            local_folders: Vec::new(),
        }
    }

    pub fn add_local_folder(&mut self, folder: BepFolder) {
        if let Some(existing) = self.local_folders.iter_mut().find(|f| f.id == folder.id) {
            *existing = folder;
        } else {
            self.local_folders.push(folder);
        }
    }

    pub fn local_folders(&self) -> &[BepFolder] {
        &self.local_folders
    }

    pub fn root_dir(&self) -> &Path {
        &self.default_root_dir
    }

    /// Sets a custom target directory for a given Syncthing `folder_id`.
    pub fn set_folder_path<P: Into<PathBuf>>(&mut self, folder_id: &str, target_path: P) {
        self.custom_folder_paths
            .insert(folder_id.to_owned(), accessible_root(target_path.into()));
    }

    /// Returns the effective root directory for a given `folder_id`.
    pub fn folder_root(&self, folder_id: &str) -> PathBuf {
        if let Some(custom) = self.custom_folder_paths.get(folder_id) {
            return custom.clone();
        }
        let clean_folder: String = folder_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        self.default_root_dir.join(clean_folder)
    }

    /// Enables or pauses a shared folder.
    pub fn set_folder_paused(&mut self, folder_id: &str, paused: bool) {
        if paused {
            self.paused_folders.insert(folder_id.to_owned());
        } else {
            self.paused_folders.remove(folder_id);
        }
    }

    pub fn is_folder_paused(&self, folder_id: &str) -> bool {
        self.paused_folders.contains(folder_id)
    }

    /// Safely resolves (without `..`) the local path of a file inside a shared folder.
    pub fn resolve_safe_path(&self, folder_id: &str, rel_path: &str) -> PathBuf {
        let mut path = self.folder_root(folder_id);
        for comp in rel_path.split(['/', '\\']) {
            if !comp.is_empty() && comp != "." && comp != ".." {
                path.push(comp);
            }
        }
        path
    }

    /// **Step 1 (Delta Sync)**:
    /// Inspects the local file block by block (fixed 128 KiB buffer in RAM) and returns
    /// only the list of blocks that are missing or have a differing SHA-256 compared to the remote `Index`.
    pub fn find_outdated_blocks(
        &self,
        folder_id: &str,
        remote_file: &BepFileInfo,
    ) -> io::Result<Vec<BepBlockInfo>> {
        let full_path = self.resolve_safe_path(folder_id, &remote_file.name);
        let meta = match fs::metadata(&full_path) {
            Ok(m) => m,
            Err(_) => return Ok(remote_file.blocks.clone()),
        };

        let mut file = match File::open(&full_path) {
            Ok(f) => f,
            Err(_) => return Ok(remote_file.blocks.clone()),
        };

        let mut outdated = Vec::new();
        let mut buf = vec![0u8; BEP_BLOCK_SIZE];

        for block in &remote_file.blocks {
            let bsize = block.size.max(0) as usize;
            if bsize > buf.len() {
                buf.resize(bsize, 0);
            }

            if file.seek(SeekFrom::Start(block.offset.max(0) as u64)).is_err() {
                outdated.push(block.clone());
                continue;
            }

            match file.read_exact(&mut buf[..bsize]) {
                Ok(()) => {
                    if block.hash.len() == 32 {
                        let actual_hash = Sha256::digest(&buf[..bsize]);
                        if actual_hash.as_slice() != block.hash.as_slice() {
                            outdated.push(block.clone());
                        }
                    }
                }
                Err(_) => outdated.push(block.clone()),
            }
        }

        if outdated.is_empty() && meta.len() != (remote_file.size.max(0) as u64) {
            if let Ok(wfile) = OpenOptions::new().write(true).open(&full_path) {
                let _ = wfile.set_len(remote_file.size.max(0) as u64);
            }
        }

        Ok(outdated)
    }

    /// Path of the local database of `folder_id`. It is keyed by the folder's current root so that
    /// pointing a share at another directory starts from a fresh database instead of advertising
    /// every previously known file as deleted.
    pub fn db_path(&self, folder_id: &str) -> PathBuf {
        let root = self.folder_root(folder_id);
        let digest = Sha256::digest(root.to_string_lossy().as_bytes());
        let clean_folder: String = folder_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        self.default_root_dir.join(format!(
            ".syncthing_db_{}_{:02x}{:02x}{:02x}{:02x}.txt",
            clean_folder, digest[0], digest[1], digest[2], digest[3]
        ))
    }

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
    pub fn commit_download(&self, folder_id: &str, rel_path: &str, final_size: u64) -> io::Result<()> {
        let full = self.resolve_safe_path(folder_id, rel_path);
        let tmp = self.download_temp_path(folder_id, rel_path);
        OpenOptions::new().write(true).open(&tmp)?.set_len(final_size)?;
        if full.exists() {
            fs::remove_file(&full)
                .map_err(|e| io::Error::new(e.kind(), format!("replace {}: {e}", full.display())))?;
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

    /// **Step 2 (Upload Block Server)**:
    /// Reads a block `[offset..offset+size]` from local storage to answer peer `BepRequest` messages.
    pub fn read_block_for_peer(
        &self,
        folder_id: &str,
        rel_path: &str,
        offset: u64,
        size: usize,
    ) -> io::Result<Vec<u8>> {
        let full_path = self.resolve_safe_path(folder_id, rel_path);
        let mut file = File::open(&full_path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; size];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// `ux0:music` -> `ux0:/music` (the form `sceIo*` always accepts), or `None` if the path is not
/// of the `device:relative` form.
fn with_device_root_slash(path: &Path) -> Option<PathBuf> {
    let s = path.to_str()?;
    let (device, rest) = s.split_once(':')?;
    let valid_device = !device.is_empty()
        && !device.contains('/')
        && device.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid_device || rest.is_empty() || rest.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(format!("{}:/{}", device, rest)))
}

/// On PS Vita, `stat()` / `opendir()` on a top-level `device:dir` path (e.g. `ux0:music`) can fail
/// although the directory exists. Prefer whichever spelling can actually be listed.
fn accessible_root(path: PathBuf) -> PathBuf {
    if fs::read_dir(&path).is_ok() {
        return path;
    }
    match with_device_root_slash(&path) {
        Some(alt) if fs::read_dir(&alt).is_ok() => alt,
        _ => path,
    }
}

/// Creates `dir` (and its parents) if needed.
///
/// On PS Vita, `sceIoMkdir` reports `EEXIST` for existing directories and `std::fs::create_dir_all`
/// then relies on `stat()` to confirm it is a directory, which can fail for some paths (e.g. names
/// normalized by exFAT). Treat the directory as present whenever it can actually be listed.
pub(crate) fn ensure_dir_exists(dir: &Path) -> io::Result<()> {
    match fs::create_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(_) if fs::read_dir(dir).is_ok() => Ok(()),
        Err(e) => Err(io::Error::new(
            e.kind(),
            format!("mkdir {}: {e}", dir.display()),
        )),
    }
}

pub(crate) fn hash_file_blocks(path: &Path) -> io::Result<Vec<BepBlockInfo>> {
    let mut file = File::open(path)?;
    let mut blocks = Vec::new();
    let mut buf = vec![0u8; BEP_BLOCK_SIZE];
    let mut offset: i64 = 0;

    loop {
        let mut read_total = 0usize;
        while read_total < BEP_BLOCK_SIZE {
            match file.read(&mut buf[read_total..]) {
                Ok(0) => break,
                Ok(n) => read_total += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }

        if read_total == 0 {
            if offset == 0 {
                let empty_hash = Sha256::digest(&[]).to_vec();
                blocks.push(BepBlockInfo {
                    offset: 0,
                    size: 0,
                    hash: empty_hash,
                });
            }
            break;
        }

        let hash = Sha256::digest(&buf[..read_total]).to_vec();
        blocks.push(BepBlockInfo {
            offset,
            size: read_total as i32,
            hash,
        });
        offset += read_total as i64;
    }

    Ok(blocks)
}

pub(crate) fn collect_files_recursive(
    base: &Path,
    prefix: &str,
    out: &mut Vec<(String, bool)>,
) -> io::Result<()> {
    for entry_res in fs::read_dir(base)? {
        let entry = entry_res?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Internal files only: download temp files, local databases and Syncthing markers.
        if name.starts_with(".syncthing") || name.starts_with(".st") {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{}/{}", prefix, name)
        };
        let path = entry.path();
        if path.is_dir() {
            out.push((rel.clone(), true));
            collect_files_recursive(&path, &rel, out)?;
        } else if path.is_file() {
            out.push((rel, false));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_root_slash_variant() {
        assert_eq!(
            with_device_root_slash(Path::new("ux0:music")),
            Some(PathBuf::from("ux0:/music"))
        );
        assert_eq!(
            with_device_root_slash(Path::new("ux0:data/syncthing")),
            Some(PathBuf::from("ux0:/data/syncthing"))
        );
        assert_eq!(with_device_root_slash(Path::new("ux0:/music")), None);
        assert_eq!(with_device_root_slash(Path::new("/tmp/a:b")), None);
        assert_eq!(with_device_root_slash(Path::new("relative/dir")), None);
    }
}
