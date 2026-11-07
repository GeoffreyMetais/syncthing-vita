//! 128 KiB block streaming storage, Delta Sync verification,
//! custom folder routing (`ux0:...`), and bidirectional local indexing.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

use crate::bep::{
    BepBlockInfo, BepFileInfo, BepFolder, BepIndex, BepVersionCounter, FolderIndexState,
    BEP_BLOCK_SIZE,
};
use crate::disco::DeviceId;

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
            .insert(folder_id.to_owned(), target_path.into());
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

    /// **Step 1 (Streaming Disk Write)**:
    /// Writes a block directly to its `offset` on disk without buffering the whole file in RAM.
    pub fn write_block_streaming(
        &self,
        folder_id: &str,
        rel_path: &str,
        offset: u64,
        data: &[u8],
        final_file_size: u64,
    ) -> io::Result<PathBuf> {
        let full_path = self.resolve_safe_path(folder_id, rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&full_path)?;

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

    /// **Step 2 (Bidirectional Local Indexing)**:
    /// Scans the local target directory for `folder_id`, chunks files into 128 KiB blocks,
    /// calculates SHA-256 checksums, and constructs the PS Vita local `BepIndex`.
    pub fn build_local_index(
        &self,
        folder_id: &str,
        own_id: DeviceId,
        remote_files: &[BepFileInfo],
    ) -> io::Result<(BepIndex, FolderIndexState)> {
        let folder_dir = self.folder_root(folder_id);
        fs::create_dir_all(&folder_dir)?;

        let short_id = u64::from_be_bytes([
            own_id.0[0],
            own_id.0[1],
            own_id.0[2],
            own_id.0[3],
            own_id.0[4],
            own_id.0[5],
            own_id.0[6],
            own_id.0[7],
        ]);

        let mut rel_entries = Vec::new();
        collect_files_recursive(&folder_dir, "", &mut rel_entries)?;
        rel_entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut files = Vec::with_capacity(rel_entries.len());
        let mut seq: i64 = 0;
        let mut index_hasher = Sha256::new();
        index_hasher.update(b"vita-bep-v2:");
        index_hasher.update(folder_id.as_bytes());
        index_hasher.update(&own_id.0);

        for (rel_name, is_dir) in rel_entries {
            let full_path = folder_dir.join(&rel_name);
            let meta = fs::metadata(&full_path)?;
            let modified_s = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(1_700_000_000);

            if is_dir {
                seq += 1;
                index_hasher.update(b"D:");
                index_hasher.update(rel_name.as_bytes());
                files.push(BepFileInfo {
                    name: rel_name,
                    is_dir: true,
                    size: 0,
                    modified_s,
                    deleted: false,
                    invalid: false,
                    sequence: seq,
                    versions: vec![BepVersionCounter {
                        short_device_id: short_id,
                        value: (modified_s.max(1) as u64),
                    }],
                    blocks: Vec::new(),
                });
                continue;
            }

            if !meta.is_file() {
                continue;
            }

            let file_size = meta.len() as i64;
            let blocks = hash_file_blocks(&full_path)?;

            let remote_match = remote_files.iter().find(|rf| rf.name == rel_name);
            let versions = match remote_match {
                Some(rf) if blocks_equal(&rf.blocks, &blocks) && rf.size == file_size => {
                    if rf.versions.is_empty() {
                        vec![BepVersionCounter {
                            short_device_id: short_id,
                            value: 1,
                        }]
                    } else {
                        rf.versions.clone()
                    }
                }
                Some(rf) => {
                    let mut v = rf.versions.clone();
                    if let Some(existing) = v.iter_mut().find(|c| c.short_device_id == short_id) {
                        existing.value += 1;
                    } else {
                        v.push(BepVersionCounter {
                            short_device_id: short_id,
                            value: (modified_s.max(1) as u64),
                        });
                    }
                    v
                }
                None => {
                    vec![BepVersionCounter {
                        short_device_id: short_id,
                        value: (modified_s.max(1) as u64),
                    }]
                }
            };

            seq += 1;
            index_hasher.update(b"F:");
            index_hasher.update(rel_name.as_bytes());
            index_hasher.update(&file_size.to_be_bytes());
            for b in &blocks {
                index_hasher.update(&b.hash);
            }

            files.push(BepFileInfo {
                name: rel_name,
                is_dir: false,
                size: file_size,
                modified_s,
                deleted: false,
                invalid: false,
                sequence: seq,
                versions,
                blocks,
            });
        }

        let digest = index_hasher.finalize();
        let index_id = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ]) | 1;

        Ok((
            BepIndex {
                folder: folder_id.to_owned(),
                files,
            },
            FolderIndexState {
                index_id,
                max_sequence: seq,
            },
        ))
    }
}

fn blocks_equal(a: &[BepBlockInfo], b: &[BepBlockInfo]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| x.size == y.size && x.hash == y.hash)
}

fn hash_file_blocks(path: &Path) -> io::Result<Vec<BepBlockInfo>> {
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

fn collect_files_recursive(
    base: &Path,
    prefix: &str,
    out: &mut Vec<(String, bool)>,
) -> io::Result<()> {
    for entry_res in fs::read_dir(base)? {
        let entry = entry_res?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
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
