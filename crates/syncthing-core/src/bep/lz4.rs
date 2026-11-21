//! Minimal pure-Rust LZ4 block decompressor for compressed BEP frames.

use std::io;

/// Largest uncompressed BEP message accepted.
const MAX_UNCOMPRESSED_LEN: usize = 32 * 1024 * 1024;

/// Decompresses an LZ4 block in Syncthing BEP format (`[u32 BE uncompressed_len][LZ4 block]`).
/// The output never grows beyond the announced `uncompressed_len` (itself capped), so a
/// crafted block cannot exhaust memory.
pub fn decompress_bep_lz4(input: &[u8]) -> io::Result<Vec<u8>> {
    if input.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "BEP LZ4 payload too short (< 4 bytes)",
        ));
    }
    let uncompressed_len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if uncompressed_len > MAX_UNCOMPRESSED_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Excessive LZ4 uncompressed size: {}", uncompressed_len),
        ));
    }

    let src = &input[4..];
    let mut out = Vec::with_capacity(uncompressed_len);
    let mut pos = 0usize;

    while pos < src.len() {
        let token = src[pos];
        pos += 1;

        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            loop {
                if pos >= src.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "LZ4 lit_len EOF",
                    ));
                }
                let b = src[pos] as usize;
                pos += 1;
                lit_len = lit_len.saturating_add(b);
                if b != 255 {
                    break;
                }
            }
        }

        if lit_len > src.len() - pos {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "LZ4 literals EOF",
            ));
        }
        if lit_len > uncompressed_len - out.len() {
            return Err(output_overflow());
        }
        out.extend_from_slice(&src[pos..pos + lit_len]);
        pos += lit_len;

        if pos >= src.len() {
            break;
        }

        if pos + 2 > src.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "LZ4 offset EOF",
            ));
        }
        let offset = u16::from_le_bytes([src[pos], src[pos + 1]]) as usize;
        pos += 2;
        if offset == 0 || offset > out.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid LZ4 offset: {offset} (out.len={})", out.len()),
            ));
        }

        let mut match_len = ((token & 0x0F) as usize) + 4;
        if (token & 0x0F) == 15 {
            loop {
                if pos >= src.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "LZ4 match_len EOF",
                    ));
                }
                let b = src[pos] as usize;
                pos += 1;
                match_len = match_len.saturating_add(b);
                if b != 255 {
                    break;
                }
            }
        }

        if match_len > uncompressed_len - out.len() {
            return Err(output_overflow());
        }
        let match_start = out.len() - offset;
        for i in 0..match_len {
            let b = out[match_start + i];
            out.push(b);
        }
    }

    Ok(out)
}

fn output_overflow() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "LZ4 block expands beyond its announced uncompressed size",
    )
}

const LZ4_HASH_BITS: usize = 12;
const LZ4_HASH_SIZE: usize = 1 << LZ4_HASH_BITS;
const LZ4_MIN_MATCH: usize = 4;
const LZ4_LAST_LITERALS: usize = 5;
const LZ4_MF_LIMIT: usize = 12;
const LZ4_MAX_OFFSET: usize = 65_535;
const LZ4_MIN_COMPRESS_LEN: usize = 128;

#[inline]
fn lz4_hash(seq: u32) -> usize {
    (seq.wrapping_mul(2_654_435_761) >> (32 - LZ4_HASH_BITS)) as usize
}

#[inline]
fn read_u32_le(src: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([src[pos], src[pos + 1], src[pos + 2], src[pos + 3]])
}

fn push_lz4_length(out: &mut Vec<u8>, mut extra: usize) {
    while extra >= 255 {
        out.push(255);
        extra -= 255;
    }
    out.push(extra as u8);
}

/// Compresses `input` in Syncthing BEP LZ4 format (`[u32 BE uncompressed_len][LZ4 block]`).
///
/// Uses a small heap-allocated hash table (`4096 * sizeof(usize)`) so PS Vita thread stacks
/// stay small. Returns `None` when `input.len() < 128`, when `input.len() > u32::MAX`, or
/// when the framed output is not strictly smaller than `input.len()`.
pub fn compress_bep_lz4(input: &[u8]) -> Option<Vec<u8>> {
    if input.len() < LZ4_MIN_COMPRESS_LEN || input.len() > u32::MAX as usize {
        return None;
    }

    let n = input.len();
    // Reserve space for the 4-byte BE uncompressed length prefix plus compressed block.
    // Abort early if output reaches `n` bytes (cannot be strictly smaller than `input.len()`).
    let mut out = Vec::with_capacity(n);
    out.extend_from_slice(&(n as u32).to_be_bytes());

    let mut hash_table = vec![0usize; LZ4_HASH_SIZE];
    let mut anchor = 0usize;

    if n >= LZ4_MF_LIMIT {
        let match_limit = n - LZ4_LAST_LITERALS;
        let mf_limit = n - LZ4_MF_LIMIT;

        // Seed position 0 and start searching from position 1 so `0` can be a real match index
        // only when `pos > candidate` and `candidate < pos`. Even at candidate=0, `pos - 0` is valid
        // as long as we check the 4 bytes match! Using `pos = 1` with `hash_table[hash(0)] = 0`
        // handles index 0 naturally without needing a sentinel.
        hash_table[lz4_hash(read_u32_le(input, 0))] = 0;
        let mut pos = 1usize;

        'outer: while pos <= mf_limit {
            let mut forward = pos;
            let mut step = 1usize;
            let mut search_match_nb = 1usize << 6;
            let mut candidate;

            loop {
                pos = forward;
                forward = pos + step;
                step = search_match_nb >> 6;
                search_match_nb += 1;

                if forward > mf_limit {
                    break 'outer;
                }

                let seq = read_u32_le(input, pos);
                let h = lz4_hash(seq);
                candidate = hash_table[h];
                hash_table[h] = pos;

                let offset = pos - candidate;
                if offset > 0 && offset <= LZ4_MAX_OFFSET && read_u32_le(input, candidate) == seq {
                    break;
                }
            }

            // Catch up backwards while bytes before `pos` and `candidate` match.
            while pos > anchor && candidate > 0 && input[pos - 1] == input[candidate - 1] {
                pos -= 1;
                candidate -= 1;
            }

            // Emit literals from `anchor..pos` and match at `candidate`.
            let lit_len = pos - anchor;
            let offset = (pos - candidate) as u16;

            // Extend match forward.
            let mut match_end = pos + LZ4_MIN_MATCH;
            let mut cand_end = candidate + LZ4_MIN_MATCH;
            while match_end < match_limit && input[match_end] == input[cand_end] {
                match_end += 1;
                cand_end += 1;
            }
            let match_len = match_end - pos - LZ4_MIN_MATCH;

            // Write token, optional extended literal length, literals, 2-byte LE offset, optional extended match length.
            let token_lit = lit_len.min(15) as u8;
            let token_match = match_len.min(15) as u8;
            out.push((token_lit << 4) | token_match);

            if lit_len >= 15 {
                push_lz4_length(&mut out, lit_len - 15);
            }
            out.extend_from_slice(&input[anchor..pos]);
            out.extend_from_slice(&offset.to_le_bytes());

            if match_len >= 15 {
                push_lz4_length(&mut out, match_len - 15);
            }

            if out.len() >= n {
                return None;
            }

            pos = match_end;
            anchor = pos;

            if pos <= mf_limit {
                // Hash position just before `pos` for better next-match density.
                let prev_seq = read_u32_le(input, pos - 2);
                hash_table[lz4_hash(prev_seq)] = pos - 2;
            }
        }
    }

    // Emit remaining trailing literals (`anchor..n`).
    let last_lit_len = n - anchor;
    let token_lit = last_lit_len.min(15) as u8;
    out.push(token_lit << 4);
    if last_lit_len >= 15 {
        push_lz4_length(&mut out, last_lit_len - 15);
    }
    out.extend_from_slice(&input[anchor..n]);

    if out.len() < n {
        Some(out)
    } else {
        None
    }
}
