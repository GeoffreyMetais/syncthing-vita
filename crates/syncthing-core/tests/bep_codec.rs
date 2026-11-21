//! BEP/1.0 wire codecs: `Hello`, `ClusterConfig`, LZ4 frames.

use syncthing_core::{decompress_bep_lz4, BepClusterConfig, BepHello, BEP_DEFAULT_NUM_CONNECTIONS};

#[test]
fn test_bep_hello_roundtrip() {
    let mut hello = BepHello::new("PS-Vita", "vita-syncthing", "0.1.0");
    assert_eq!(hello.num_connections, BEP_DEFAULT_NUM_CONNECTIONS);
    hello.timestamp = 1_700_000_000_123_456_789;
    let frame = hello.encode_frame();
    assert_eq!(&frame[0..4], &[0x2E, 0xA7, 0xD9, 0x0B]);

    let decoded = BepHello::decode_frame(&frame).expect("BEP Hello decoding failed");
    assert_eq!(decoded, hello);
    assert_eq!(decoded.num_connections, 3);
    assert_eq!(decoded.timestamp, 1_700_000_000_123_456_789);
}

#[test]
fn test_bep_secondary_cluster_config_roundtrip() {
    let sec_bytes = BepClusterConfig::encode_secondary();
    let decoded =
        BepClusterConfig::decode(&sec_bytes).expect("Secondary ClusterConfig decode failed");
    assert!(decoded.secondary);
    assert!(decoded.folders.is_empty());
}

#[test]
fn test_lz4_decompression() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&12u32.to_be_bytes());
    payload.push(0x80);
    payload.extend_from_slice(b"VITA_BEP");
    payload.extend_from_slice(&8u16.to_le_bytes());
    let decompressed = decompress_bep_lz4(&payload).expect("LZ4 decompression failed");
    assert_eq!(&decompressed, b"VITA_BEPVITA");
}

#[test]
fn test_lz4_output_is_bounded_by_announced_size() {
    // 16 bytes announced, but the match length extension expands to about 1 MiB.
    let mut bomb = vec![0, 0, 0, 16, 0x1F, b'A', 1, 0];
    bomb.extend(std::iter::repeat_n(255u8, 4000));
    bomb.push(0);
    assert!(decompress_bep_lz4(&bomb).is_err());

    // Literals longer than the announced size are rejected too.
    let mut literals = vec![0, 0, 0, 2, 0x40];
    literals.extend_from_slice(b"VITA");
    assert!(decompress_bep_lz4(&literals).is_err());
}

/// A symlink entry (BEP type 4) is decoded as invalid: it is never pulled as an empty file.
#[test]
fn test_symlink_entries_are_decoded_as_invalid() {
    use syncthing_core::proto::{encode_bytes_field, encode_varint_field};
    use syncthing_core::BepIndex;
    let entry = |file_type: u64| {
        let mut file = Vec::new();
        encode_bytes_field(&mut file, 1, b"link");
        encode_varint_field(&mut file, 2, file_type);
        let mut index = Vec::new();
        encode_bytes_field(&mut index, 1, b"f");
        encode_bytes_field(&mut index, 2, &file);
        BepIndex::decode(&index).unwrap().files.remove(0)
    };
    assert!(!entry(0).invalid && !entry(0).is_dir);
    assert!(!entry(1).invalid && entry(1).is_dir);
    for symlink in [2, 3, 4] {
        assert!(entry(symlink).invalid && !entry(symlink).is_dir);
    }
}

/// The announced `block_size` (field 13) is the size of the file's blocks, not always 128 KiB:
/// a file pulled from a peer keeps the (larger) blocks that peer chose.
#[test]
fn test_announced_block_size_follows_the_blocks() {
    use syncthing_core::proto::{decode_varint, WireIter};
    use syncthing_core::{BepBlockInfo, BepFileInfo, BepIndex, BEP_BLOCK_SIZE};
    let announced = |block_sizes: &[i32]| -> u64 {
        let mut offset = 0;
        let blocks: Vec<BepBlockInfo> = block_sizes
            .iter()
            .map(|&size| {
                let block = BepBlockInfo {
                    offset,
                    size,
                    hash: vec![0; 32],
                    weak_hash: 0,
                };
                offset += i64::from(size);
                block
            })
            .collect();
        let index = BepIndex {
            folder: "f".to_owned(),
            files: vec![BepFileInfo {
                name: "a.bin".to_owned(),
                is_dir: false,
                size: offset,
                modified_s: 1,
                deleted: false,
                invalid: false,
                sequence: 1,
                versions: Vec::new(),
                blocks,
            }],
        };
        let payload = index.encode();
        let file = WireIter::new(&payload)
            .filter_map(Result::ok)
            .find(|(field, _, _)| *field == 2)
            .unwrap()
            .2;
        let size = WireIter::new(file)
            .filter_map(Result::ok)
            .find(|(field, _, _)| *field == 13)
            .unwrap()
            .2;
        decode_varint(size).unwrap().0
    };
    let mib = 1024 * 1024;
    assert_eq!(announced(&[mib, mib, 5]), mib as u64);
    assert_eq!(
        announced(&[BEP_BLOCK_SIZE as i32, 7]),
        BEP_BLOCK_SIZE as u64
    );
    assert_eq!(announced(&[42]), BEP_BLOCK_SIZE as u64);
    assert_eq!(announced(&[]), BEP_BLOCK_SIZE as u64);
}
