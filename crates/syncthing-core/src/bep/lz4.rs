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
