//! LZ4 block decompression of BEP frames, checked with hand-built blocks: literal and match
//! length extensions, offsets, and every bound on the announced uncompressed size.

use std::io::ErrorKind;

use syncthing_core::decompress_bep_lz4;

/// Prefixes an LZ4 block with the big-endian uncompressed length, as BEP does.
fn bep(uncompressed_len: u32, block: &[u8]) -> Vec<u8> {
    let mut out = uncompressed_len.to_be_bytes().to_vec();
    out.extend_from_slice(block);
    out
}

#[test]
fn literal_only_block_is_copied() {
    // token 0x30: 3 literals, no match (last sequence).
    let input = bep(3, &[0x30, b'a', b'b', b'c']);
    assert_eq!(decompress_bep_lz4(&input).unwrap(), b"abc");
}

#[test]
fn announced_length_of_exactly_32_mib_is_accepted() {
    // 32 MiB = 0x0200_0000. An empty block produces nothing.
    let input = [0x02, 0x00, 0x00, 0x00];
    assert_eq!(decompress_bep_lz4(&input).unwrap(), Vec::<u8>::new());
}

#[test]
fn announced_length_above_32_mib_is_rejected() {
    let input = [0x02, 0x00, 0x00, 0x01];
    let err = decompress_bep_lz4(&input).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert!(err.to_string().contains("Excessive"));
}

#[test]
fn input_shorter_than_the_length_prefix_is_rejected() {
    let err = decompress_bep_lz4(&[0, 0, 1]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
}

#[test]
fn literal_length_extension_is_summed_exactly() {
    // token 0xF0: literal length 15 + 255 + 1 = 271.
    let mut block = vec![0xF0, 255, 1];
    block.extend(std::iter::repeat_n(b'x', 271));
    let expected = vec![b'x'; 271];
    assert_eq!(decompress_bep_lz4(&bep(271, &block)).unwrap(), expected);
}

#[test]
fn literal_run_that_exactly_fills_the_input_is_accepted() {
    let input = bep(2, &[0x20, b'a', b'b']);
    assert_eq!(decompress_bep_lz4(&input).unwrap(), b"ab");
}

#[test]
fn literal_run_past_the_end_of_input_is_rejected() {
    // Three literals announced, only two bytes follow.
    let err = decompress_bep_lz4(&bep(3, &[0x30, b'a', b'b'])).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
}

#[test]
fn literals_beyond_the_announced_size_are_rejected() {
    let err = decompress_bep_lz4(&bep(2, &[0x30, b'a', b'b', b'c'])).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert!(err.to_string().contains("expands beyond"));
}

#[test]
fn offset_is_required_after_literals() {
    // One literal, then only one byte where the 2-byte offset should be.
    let err = decompress_bep_lz4(&bep(2, &[0x10, b'a', 0x01])).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    assert!(err.to_string().contains("offset EOF"));
}

#[test]
fn match_with_exact_offset_bytes_is_decoded() {
    // literal 'a'; match offset 1, length 4 -> five 'a'.
    let input = bep(5, &[0x10, b'a', 0x01, 0x00]);
    assert_eq!(decompress_bep_lz4(&input).unwrap(), b"aaaaa");
}

#[test]
fn zero_offset_is_rejected() {
    let err = decompress_bep_lz4(&bep(5, &[0x10, b'a', 0x00, 0x00])).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert!(err.to_string().contains("Invalid LZ4 offset"));
}

#[test]
fn offset_beyond_the_output_is_rejected() {
    // Only one byte is in the output, the offset 2 points before its start.
    let err = decompress_bep_lz4(&bep(5, &[0x10, b'a', 0x02, 0x00])).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn sequences_advance_past_the_offset_bytes() {
    // Sequence 1: 2 literals "xy", match offset 1 length 4 -> "xyyyyy".
    // Sequence 2 starts right after the offset: 1 literal "z".
    let input = bep(7, &[0x20, b'x', b'y', 0x01, 0x00, 0x10, b'z']);
    assert_eq!(decompress_bep_lz4(&input).unwrap(), b"xyyyyyz");
}

#[test]
fn match_length_extension_is_summed_exactly() {
    // token 0x1F: literal 1, match nibble 15 -> match length 19 + 255 + 3 = 277.
    let input = bep(278, &[0x1F, b'a', 0x01, 0x00, 0xFF, 0x03]);
    assert_eq!(decompress_bep_lz4(&input).unwrap(), vec![b'a'; 278]);
}

#[test]
fn match_beyond_the_announced_size_is_rejected() {
    // Literal 'a' plus a 4-byte match would produce 5 bytes, but only 4 are announced.
    let err = decompress_bep_lz4(&bep(4, &[0x10, b'a', 0x01, 0x00])).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert!(err.to_string().contains("expands beyond"));
}

#[test]
fn truncated_match_length_extension_is_rejected() {
    // Match nibble 15 requires an extension byte that is missing.
    let err = decompress_bep_lz4(&bep(5, &[0x1F, b'a', 0x01, 0x00])).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
}

#[test]
fn output_never_exceeds_the_announced_size_for_overlapping_matches() {
    // A long overlapping match with a small announced size is refused, not expanded.
    let mut block = vec![0x1F, b'a', 0x01, 0x00];
    block.extend(std::iter::repeat_n(255u8, 4000));
    block.push(0);
    assert!(decompress_bep_lz4(&bep(16, &block)).is_err());
}
