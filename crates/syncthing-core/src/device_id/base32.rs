//! RFC 4648 Base32 and Luhn mod 32 primitives used by Syncthing Device IDs.

pub(super) const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Encodes 32 bytes into RFC 4648 Base32 without padding (52 characters for 256 bits).
pub(super) fn base32_encode(data: &[u8; 32]) -> String {
    let mut out = String::with_capacity(52);
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;

    for &byte in data.iter() {
        buffer = (buffer << 8) | (byte as u32);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1F) as usize;
            out.push(BASE32_ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        out.push(BASE32_ALPHABET[idx] as char);
    }
    out
}

/// Decodes Base32 characters (`A-Z`, `2-7`) into `out` (MSB-first, 5 bits per character).
pub(super) fn base32_decode_prefix(b32_chars: &[u8], out: &mut [u8]) -> Option<()> {
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;
    let mut out_idx: usize = 0;

    for &ch in b32_chars {
        let val = BASE32_ALPHABET.iter().position(|&b| b == ch)? as u32;
        buffer = (buffer << 5) | val;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            if out_idx < out.len() {
                out[out_idx] = ((buffer >> bits_left) & 0xFF) as u8;
                out_idx += 1;
            }
        }
    }
    if bits_left > 0 && out_idx < out.len() {
        out[out_idx] = ((buffer << (8 - bits_left)) & 0xFF) as u8;
    }
    Some(())
}

/// Computes the Luhn mod 32 check character used by Syncthing Device IDs.
pub(super) fn luhn_mod32_check_char(chunk: &[u8]) -> u8 {
    let mut factor = 1usize;
    let mut sum = 0usize;
    let n = 32usize;

    for &ch in chunk {
        let code = BASE32_ALPHABET.iter().position(|&b| b == ch).unwrap_or(0);
        let mut addend = factor * code;
        factor = if factor == 2 { 1 } else { 2 };
        addend = (addend / n) + (addend % n);
        sum += addend;
    }

    let remainder = sum % n;
    let check_code = (n - remainder) % n;
    BASE32_ALPHABET[check_code]
}
