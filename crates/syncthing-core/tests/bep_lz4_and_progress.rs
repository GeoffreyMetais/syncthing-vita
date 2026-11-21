//! Unit & integration tests for BEP LZ4 send compression, `BepDeviceCompression`, and
//! `DownloadProgress` (type 5).

mod common;

use std::io::Cursor;
use std::time::Duration;

use common::fake_peer::FakePeer;
use common::fresh_temp_dir;
use common::session_driver::{device_under_test, hello_under_test, payloads, run_session};
use syncthing_core::proto::{decode_varint, WireIter};
use syncthing_core::{
    compress_bep_lz4, decompress_bep_lz4, read_bep_message, write_bep_message_compressed,
    BepClusterConfig, BepCompression, BepDeviceCompression, BepDownloadProgress,
    BepFileDownloadProgressUpdate, BepFolder, BepHeader, BepMessageType, BepVersionCounter,
    DeviceId, DownloadProgressUpdateType, FolderIndexState, FsSyncStorage, BEP_BLOCK_SIZE,
};

#[test]
fn lz4_roundtrip_repetitive_and_text_and_binary() {
    let repetitive = vec![b'A'; 4096];
    let compressed = compress_bep_lz4(&repetitive).expect("repetitive payload must compress");
    assert!(compressed.len() < repetitive.len());
    assert_eq!(decompress_bep_lz4(&compressed).unwrap(), repetitive);

    let text_pattern = b"syncthing-vita BEP/1.0 index metadata entry path=/ux0/data/file.txt ";
    let mut text = Vec::new();
    for _ in 0..80 {
        text.extend_from_slice(text_pattern);
    }
    let compressed_text = compress_bep_lz4(&text).expect("repetitive text must compress");
    assert!(compressed_text.len() < text.len());
    assert_eq!(decompress_bep_lz4(&compressed_text).unwrap(), text);

    // Structured binary payload with repeating 64-byte blocks and varied bytes
    let mut binary = Vec::with_capacity(8192);
    for i in 0..128u32 {
        binary.extend_from_slice(&i.to_le_bytes());
        for _ in 0..15 {
            binary.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        }
    }
    let compressed_bin = compress_bep_lz4(&binary).expect("structured binary must compress");
    assert!(compressed_bin.len() < binary.len());
    assert_eq!(decompress_bep_lz4(&compressed_bin).unwrap(), binary);
}

#[test]
fn lz4_edge_lengths_and_128kib() {
    assert_eq!(compress_bep_lz4(&[]), None);
    assert_eq!(compress_bep_lz4(&[0x42; 127]), None);

    let exact_128 = vec![0x42; 128];
    let c128 = compress_bep_lz4(&exact_128).expect("128 identical bytes must compress");
    assert!(c128.len() < exact_128.len());
    assert_eq!(decompress_bep_lz4(&c128).unwrap(), exact_128);

    // 128 KiB block (standard BEP block size) with periodic variations
    let mut block_128k = Vec::with_capacity(BEP_BLOCK_SIZE);
    for i in 0..(BEP_BLOCK_SIZE / 32) {
        block_128k.extend_from_slice(&(i as u32 % 16).to_be_bytes());
        block_128k.extend_from_slice(&[0x55; 28]);
    }
    let c_block = compress_bep_lz4(&block_128k).expect("128 KiB block must compress");
    assert!(c_block.len() < block_128k.len());
    assert_eq!(decompress_bep_lz4(&c_block).unwrap(), block_128k);
}

#[test]
fn lz4_incompressible_data_returns_none() {
    // Deterministic xorshift32 pseudo-random bytes (high entropy, incompressible)
    let mut rng = 0x1234_5678u32;
    let mut data = Vec::with_capacity(4096);
    for _ in 0..4096 {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        data.push((rng >> 24) as u8);
    }
    assert_eq!(compress_bep_lz4(&data), None);
}

#[test]
fn write_bep_message_compressed_selects_lz4_or_none_and_roundtrips() {
    let compressible = vec![b'Z'; 1024];
    let mut buf = Vec::new();
    write_bep_message_compressed(&mut buf, BepMessageType::Index, &compressible).unwrap();

    // Verify wire header uses BepCompression::Lz4
    let hdr_len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    let hdr = BepHeader::decode(&buf[2..2 + hdr_len]).unwrap();
    assert_eq!(hdr.msg_type, BepMessageType::Index);
    assert_eq!(hdr.compression, BepCompression::Lz4);

    let (msg_type, decoded) = read_bep_message(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(msg_type, BepMessageType::Index);
    assert_eq!(decoded, compressible);

    // Small payload (< 128 bytes) falls back cleanly to BepCompression::None
    let small = b"short-index-payload".to_vec();
    let mut small_buf = Vec::new();
    write_bep_message_compressed(&mut small_buf, BepMessageType::IndexUpdate, &small).unwrap();

    let hdr_len = u16::from_be_bytes([small_buf[0], small_buf[1]]) as usize;
    let hdr = BepHeader::decode(&small_buf[2..2 + hdr_len]).unwrap();
    assert_eq!(hdr.msg_type, BepMessageType::IndexUpdate);
    assert_eq!(hdr.compression, BepCompression::None);

    let (msg_type, decoded) = read_bep_message(&mut Cursor::new(&small_buf)).unwrap();
    assert_eq!(msg_type, BepMessageType::IndexUpdate);
    assert_eq!(decoded, small);

    // High-entropy payload (>= 128 bytes) falls back cleanly to BepCompression::None
    let mut rng = 0xCAFE_BABEu32;
    let mut random = Vec::with_capacity(512);
    for _ in 0..512 {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        random.push((rng & 0xFF) as u8);
    }
    let mut rnd_buf = Vec::new();
    write_bep_message_compressed(&mut rnd_buf, BepMessageType::Response, &random).unwrap();

    let hdr_len = u16::from_be_bytes([rnd_buf[0], rnd_buf[1]]) as usize;
    let hdr = BepHeader::decode(&rnd_buf[2..2 + hdr_len]).unwrap();
    assert_eq!(hdr.compression, BepCompression::None);

    let (msg_type, decoded) = read_bep_message(&mut Cursor::new(&rnd_buf)).unwrap();
    assert_eq!(msg_type, BepMessageType::Response);
    assert_eq!(decoded, random);
}

#[test]
fn download_progress_update_and_forget_roundtrip() {
    let update_msg = BepDownloadProgress {
        folder: "vita-saves".to_owned(),
        updates: vec![BepFileDownloadProgressUpdate {
            update_type: DownloadProgressUpdateType::Update,
            name: "save/slot0.bin".to_owned(),
            version: vec![
                BepVersionCounter {
                    short_device_id: 0x0102_0304_0506_0708,
                    value: 42,
                },
                BepVersionCounter {
                    short_device_id: 0x1122_3344_5566_7788,
                    value: 3,
                },
            ],
            block_indexes: vec![0, 1, 2, 3, 4, 5, 6, 7],
            block_size: BEP_BLOCK_SIZE as i32,
        }],
    };
    let encoded = update_msg.encode();
    let decoded = BepDownloadProgress::decode(&encoded).expect("decode Update");
    assert_eq!(decoded, update_msg);

    // Check that field 4 inside the FileDownloadProgressUpdate uses packed wire type 2
    let updates_slice = WireIter::new(&encoded)
        .map(Result::unwrap)
        .find(|(f, w, _)| *f == 2 && *w == 2)
        .expect("updates field")
        .2;
    let block_indexes_wire = WireIter::new(updates_slice)
        .map(Result::unwrap)
        .find(|(f, _, _)| *f == 4)
        .expect("block_indexes field")
        .1;
    assert_eq!(block_indexes_wire, 2, "packed repeated int32 wire type");

    let forget_msg = BepDownloadProgress {
        folder: "vita-saves".to_owned(),
        updates: vec![BepFileDownloadProgressUpdate {
            update_type: DownloadProgressUpdateType::Forget,
            name: "save/slot0.bin".to_owned(),
            version: vec![BepVersionCounter {
                short_device_id: 0x0102_0304_0506_0708,
                value: 42,
            }],
            block_indexes: Vec::new(),
            block_size: BEP_BLOCK_SIZE as i32,
        }],
    };
    let forget_encoded = forget_msg.encode();
    let forget_decoded = BepDownloadProgress::decode(&forget_encoded).expect("decode Forget");
    assert_eq!(forget_decoded, forget_msg);
}

#[test]
fn cluster_config_device_compression_roundtrip() {
    let own = DeviceId([0xAA; 32]);
    let peer = DeviceId([0xBB; 32]);
    let folders = vec![(
        BepFolder {
            id: "docs".to_owned(),
            label: "Docs".to_owned(),
            device_ids: vec![own, peer],
        },
        FolderIndexState::default(),
    )];

    for pref in [
        BepDeviceCompression::Metadata,
        BepDeviceCompression::Never,
        BepDeviceCompression::Always,
    ] {
        let encoded =
            BepClusterConfig::encode_for_peers_with_compression(&folders, own, "vita", peer, pref);
        assert_eq!(
            BepClusterConfig::decode_peer_compression(&encoded, &own),
            pref
        );
        assert_eq!(
            BepClusterConfig::decode_peer_compression(&encoded, &peer),
            pref
        );
    }

    // Unknown peer defaults to Metadata
    let default_encoded = BepClusterConfig::encode_for_peers(&folders, own, "vita", peer);
    assert_eq!(
        BepClusterConfig::decode_peer_compression(&default_encoded, &DeviceId([0xCC; 32])),
        BepDeviceCompression::Metadata
    );

    // Verify field 4 varint value inside encoded device entry for Always (= 2)
    let always_encoded = BepClusterConfig::encode_for_peers_with_compression(
        &folders,
        own,
        "vita",
        peer,
        BepDeviceCompression::Always,
    );
    let folder_bytes = WireIter::new(&always_encoded)
        .map(Result::unwrap)
        .find(|(f, _, _)| *f == 1)
        .unwrap()
        .2;
    let dev_bytes = WireIter::new(folder_bytes)
        .map(Result::unwrap)
        .find(|(f, _, _)| *f == 16)
        .unwrap()
        .2;
    let comp_val = WireIter::new(dev_bytes)
        .map(Result::unwrap)
        .find(|(f, w, _)| *f == 4 && *w == 0)
        .map(|(_, _, d)| decode_varint(d).unwrap().0);
    assert_eq!(comp_val, Some(2));
}

#[test]
fn live_session_compressed_index_and_multiblock_download_progress() {
    let dir = fresh_temp_dir("p2_1_live_lz4_progress");
    let folder_dir = dir.join("shared");
    std::fs::create_dir_all(&folder_dir).unwrap();
    // Create local files so the device's outgoing Index is > 128 bytes and compressible
    for i in 0..8 {
        std::fs::write(
            folder_dir.join(format!("local_repetitive_{i:02}.txt")),
            format!("local file content {i}"),
        )
        .unwrap();
    }

    let own = device_under_test();
    // 10 blocks of 128 KiB -> triggers Update after 8 blocks and Forget on completion
    let multi_block_size = 10 * BEP_BLOCK_SIZE;
    let remote_content = vec![0x5Au8; multi_block_size];
    let peer_content = remote_content.clone();

    let peer = FakePeer::spawn(91, move |conn| {
        conn.send_cluster_config(&["shared"], own.device_id);
        let remote_file =
            conn.file_in_blocks("shared", "big_file.bin", &peer_content, 1, BEP_BLOCK_SIZE);
        conn.send_index("shared", vec![remote_file], false);

        // Serve blocks while collecting DownloadProgress and the device's Index
        conn.serve_until_idle(Duration::from_millis(800));
    });

    let mut storage = FsSyncStorage::new(&dir);
    storage.set_folder_path("shared", &folder_dir);
    storage.set_folder_devices("shared", vec![peer.device_id]);
    storage.add_local_folder(BepFolder {
        id: "shared".to_owned(),
        label: "Shared".to_owned(),
        device_ids: vec![own.device_id, peer.device_id],
    });

    run_session(&peer, &storage, &hello_under_test(1), None, |_| {}).expect("sync session");
    let received = peer.join();

    // Multi-block file downloaded intact
    assert_eq!(
        std::fs::read(folder_dir.join("big_file.bin")).unwrap(),
        remote_content
    );

    // Local Index was published and decoded cleanly (compressed on wire, decompressed by FakePeer)
    let sent_indexes = payloads(&received, BepMessageType::Index);
    assert!(!sent_indexes.is_empty(), "expected local Index message");

    // DownloadProgress messages: at least one Update (8 blocks) and one Forget (on completion)
    let progress_payloads = payloads(&received, BepMessageType::DownloadProgress);
    assert!(
        progress_payloads.len() >= 2,
        "expected Update + Forget DownloadProgress messages, got {}",
        progress_payloads.len()
    );

    let decoded_progress: Vec<BepDownloadProgress> = progress_payloads
        .into_iter()
        .map(|p| BepDownloadProgress::decode(p).expect("valid DownloadProgress"))
        .collect();

    assert!(decoded_progress.iter().any(|dp| {
        dp.folder == "shared"
            && dp.updates.iter().any(|u| {
                u.update_type == DownloadProgressUpdateType::Update
                    && u.name == "big_file.bin"
                    && u.block_indexes == vec![0, 1, 2, 3, 4, 5, 6, 7]
            })
    }));

    assert!(decoded_progress.iter().any(|dp| {
        dp.folder == "shared"
            && dp.updates.iter().any(|u| {
                u.update_type == DownloadProgressUpdateType::Forget
                    && u.name == "big_file.bin"
                    && u.block_indexes.is_empty()
            })
    }));
}
