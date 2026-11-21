//! Rolling Adler-32 weak hash (`weak_hash`) for shifted-block reuse.
//!
//! Syncthing uses a rolling Adler-32 checksum with `MOD_ADLER = 65521` and initial state
//! `a = 0, b = 0` (matching Syncthing's `lib/weakhash` / `chmduquesne/rollinghash/adler32`
//! with zero-initialized accumulators rather than RFC 1950's `a = 1, b = 0`).
//!
//! Every `BepBlockInfo` can carry a 32-bit `weak_hash` computed over its exact block bytes:
//! `((b as u32) << 16) | (a as u32)` where after processing `data[0..n]`:
//! - `a = (sum_{i=0..n-1} data[i]) mod 65521`
//! - `b = (sum_{i=0..n-1} (n - i) * data[i]) mod 65521`

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use crate::bep::{BepBlockInfo, BEP_BLOCK_HASH_LEN, BEP_MAX_BLOCK_SIZE};

/// Largest prime smaller than `2^16`.
pub const MOD_ADLER: u32 = 65521;

/// Maximum number of candidate byte offsets recorded per `weak_hash` during a streaming file
/// scan (bounds memory on PS Vita and matches Syncthing's false-positive cap per weak hash).
const MAX_OFFSETS_PER_WEAK_HASH: usize = 10;

/// Computes Syncthing's Adler-32 weak hash (`a = 0, b = 0` initial state) over `data`.
pub fn weak_hash(data: &[u8]) -> u32 {
    let mut a: u32 = 0;
    let mut b: u32 = 0;
    // Process in chunks small enough that `a` and `b` never overflow `u32` before `% MOD_ADLER`.
    // Max `a` increase over 5552 bytes: 5552 * 255 = 1_415_760; max `b`: ~3.9e9 < u32::MAX.
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD_ADLER;
        b %= MOD_ADLER;
    }
    (b << 16) | a
}

/// Rolling Adler-32 window of fixed size `window_len` (`a = 0, b = 0` initial state).
#[derive(Debug, Clone)]
pub struct RollingWeakHash {
    window_len: u32,
    a: u32,
    b: u32,
}

impl RollingWeakHash {
    /// Initializes the rolling checksum over the first window `initial`.
    pub fn new(initial: &[u8]) -> Self {
        let mut a: u32 = 0;
        let mut b: u32 = 0;
        for chunk in initial.chunks(5552) {
            for &byte in chunk {
                a += u32::from(byte);
                b += a;
            }
            a %= MOD_ADLER;
            b %= MOD_ADLER;
        }
        Self {
            window_len: (initial.len() as u32) % MOD_ADLER,
            a,
            b,
        }
    }

    /// Returns the current 32-bit weak hash `((b << 16) | a)`.
    pub fn sum32(&self) -> u32 {
        (self.b << 16) | self.a
    }

    /// Advances the window by one byte (`out_byte` leaving at the front, `in_byte` entering at
    /// the back) and returns the updated 32-bit weak hash.
    pub fn roll(&mut self, out_byte: u8, in_byte: u8) -> u32 {
        let out = u32::from(out_byte);
        let inp = u32::from(in_byte);
        self.a = (self.a + MOD_ADLER + inp - out) % MOD_ADLER;
        let sub_b = (self.window_len * out) % MOD_ADLER;
        self.b = (self.b + MOD_ADLER + self.a - sub_b) % MOD_ADLER;
        self.sum32()
    }
}

/// Searches `file` for a single contiguous block of `block_size` bytes whose rolling `weak_hash`
/// equals `target_weak` and whose SHA-256 equals `target_sha256`.
///
/// Uses a bounded ring/sliding buffer in memory (at most `2 * block_size` bytes) so it stays
/// safe on PS Vita for large files.
pub fn find_shifted_block(
    file: &mut File,
    block_size: usize,
    target_weak: u32,
    target_sha256: &[u8],
) -> io::Result<Option<Vec<u8>>> {
    if block_size == 0
        || block_size > BEP_MAX_BLOCK_SIZE
        || target_weak == 0
        || target_sha256.len() != BEP_BLOCK_HASH_LEN
    {
        return Ok(None);
    }

    let mut targets = HashMap::with_capacity(1);
    targets.insert(target_weak, vec![target_sha256.to_vec()]);

    let matches = find_shifted_blocks_in_file(file, block_size, &targets)?;
    let Some(offset) = matches.get(target_sha256).copied() else {
        return Ok(None);
    };

    let mut block = vec![0u8; block_size];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut block)?;
    Ok(Some(block))
}

/// Scans `file` in a single streaming pass with a rolling window of `block_size` bytes and
/// returns `sha256 -> file_offset` for every target block in `targets` (`weak_hash -> sha256s`)
/// found anywhere in `file`.
///
/// Memory is bounded by `2 * block_size` for the sliding read buffer plus at most
/// [`MAX_OFFSETS_PER_WEAK_HASH`] candidate hits verified inline via SHA-256.
pub fn find_shifted_blocks_in_file(
    file: &mut File,
    block_size: usize,
    targets: &HashMap<u32, Vec<Vec<u8>>>,
) -> io::Result<HashMap<Vec<u8>, u64>> {
    let mut found: HashMap<Vec<u8>, u64> = HashMap::new();
    if block_size == 0 || block_size > BEP_MAX_BLOCK_SIZE || targets.is_empty() {
        return Ok(found);
    }

    let mut remaining: HashSet<&[u8]> = targets
        .values()
        .flat_map(|hashes| hashes.iter().map(Vec::as_slice))
        .filter(|h| h.len() == BEP_BLOCK_HASH_LEN)
        .collect();
    if remaining.is_empty() {
        return Ok(found);
    }

    file.seek(SeekFrom::Start(0))?;
    // Hold two `block_size` regions so we can roll across chunk boundaries without seeking.
    let mut buf = vec![0u8; block_size * 2];
    let filled = read_full_or_eof(file, &mut buf[..block_size])?;
    if filled < block_size {
        return Ok(found);
    }

    let mut rolling = RollingWeakHash::new(&buf[..block_size]);
    let mut weak_hits: HashMap<u32, usize> = HashMap::new();

    verify_window(
        &buf[..block_size],
        0,
        rolling.sum32(),
        targets,
        &mut weak_hits,
        &mut remaining,
        &mut found,
    );
    if remaining.is_empty() {
        return Ok(found);
    }

    let mut base_offset: u64 = 0;
    loop {
        let extra = read_full_or_eof(file, &mut buf[block_size..])?;
        if extra == 0 {
            break;
        }
        for i in 0..extra {
            let out_byte = buf[i];
            let in_byte = buf[i + block_size];
            let weak = rolling.roll(out_byte, in_byte);
            let window_start = i + 1;
            verify_window(
                &buf[window_start..window_start + block_size],
                base_offset + window_start as u64,
                weak,
                targets,
                &mut weak_hits,
                &mut remaining,
                &mut found,
            );
            if remaining.is_empty() {
                return Ok(found);
            }
        }
        if extra < block_size {
            break;
        }
        buf.copy_within(block_size..block_size * 2, 0);
        base_offset += block_size as u64;
    }

    Ok(found)
}

/// Computes `weak_hash` for each block in `blocks` that still needs one, using a single pass
/// over `file` when needed, or returns `blocks` unmodified if already populated.
pub fn populate_missing_weak_hashes(
    blocks: &[BepBlockInfo],
    path: &std::path::Path,
) -> io::Result<Vec<BepBlockInfo>> {
    if blocks.iter().all(|b| b.size == 0 || b.weak_hash != 0) {
        return Ok(blocks.to_vec());
    }
    let mut file = File::open(path)?;
    let mut out = blocks.to_vec();
    let mut buf = Vec::new();
    for block in &mut out {
        if block.size <= 0 || block.size as usize > BEP_MAX_BLOCK_SIZE {
            continue;
        }
        let len = block.size as usize;
        if buf.len() < len {
            buf.resize(len, 0);
        }
        file.seek(SeekFrom::Start(block.offset.max(0) as u64))?;
        file.read_exact(&mut buf[..len])?;
        block.weak_hash = weak_hash(&buf[..len]);
    }
    Ok(out)
}

fn verify_window(
    window: &[u8],
    offset: u64,
    weak: u32,
    targets: &HashMap<u32, Vec<Vec<u8>>>,
    weak_hits: &mut HashMap<u32, usize>,
    remaining: &mut HashSet<&[u8]>,
    found: &mut HashMap<Vec<u8>, u64>,
) {
    let Some(candidate_hashes) = targets.get(&weak) else {
        return;
    };
    let hits = weak_hits.entry(weak).or_insert(0);
    if *hits >= MAX_OFFSETS_PER_WEAK_HASH {
        return;
    }
    *hits += 1;

    let digest = Sha256::digest(window);
    let digest_slice = digest.as_slice();
    for expected in candidate_hashes {
        if expected.as_slice() == digest_slice && remaining.remove(digest_slice) {
            found.insert(expected.clone(), offset);
        }
    }
}

fn read_full_or_eof(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match file.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}
