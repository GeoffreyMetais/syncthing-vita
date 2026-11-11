//! Block-level file access: SHA-256 block hashing, delta comparison against a remote file and
//! block reads served to peers. Uses a fixed 128 KiB buffer (never the whole file in RAM).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use sha2::{Digest, Sha256};

use super::FsSyncStorage;
use crate::bep::{BepBlockInfo, BepFileInfo, BEP_BLOCK_SIZE};

impl FsSyncStorage {
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

            if file
                .seek(SeekFrom::Start(block.offset.max(0) as u64))
                .is_err()
            {
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

/// Splits a file into 128 KiB blocks and returns their offsets, sizes and SHA-256 hashes.
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
