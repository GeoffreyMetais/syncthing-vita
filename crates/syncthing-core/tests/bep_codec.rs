//! BEP/1.0 wire codecs: `Hello`, `ClusterConfig`, LZ4 frames.

use syncthing_core::{decompress_bep_lz4, BepClusterConfig, BepHello, BEP_DEFAULT_NUM_CONNECTIONS};

#[test]
fn test_bep_hello_roundtrip() {
    let mut hello = BepHello::new_vita_default();
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
