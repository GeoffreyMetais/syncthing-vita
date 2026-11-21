//! Tests for Milestone P2.3:
//! - Rolling Adler-32 `weak_hash` (`a = 0, b = 0` initial state) matching fresh computation at
//!   every byte offset across a buffer.
//! - `BepBlockInfo` (field 4) and `BepRequest` (field 8) `weak_hash` Protobuf roundtrips.
//! - Shifted-block reuse during delta sync (prepending bytes shifts every 128 KiB block boundary,
//!   yet the receiver reuses local blocks via `weak_hash` + SHA-256 verification without
//!   re-downloading the shifted blocks over the network).
//! - Inbound secondary `1+2` connection attachment via `SecondaryInbox` (peer dials primary +
//!   2 secondaries; acceptor promotes both secondaries so `active_connections == 3` and syncs).

mod common;

use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use syncthing_core::bep::{
    BepBlockInfo, BepFileInfo, BepIndex, BepRequest, BepVersionCounter, BEP_BLOCK_SIZE,
};
use syncthing_core::storage::weak_hash::{weak_hash, RollingWeakHash};
use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, accept_secondary_cluster_config, connect_and_sync,
    BepFolder, BepHello, DeviceIdentity, FsSyncStorage, SecondaryInbox, SessionParams, SyncControl,
    SyncEvent, SyncSessionStatus, SyncTuning,
};

const TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn test_rolling_weak_hash_matches_fresh_computation_at_every_offset() {
    let mut data = vec![0u8; 256];
    for (i, b) in data.iter_mut().enumerate() {
        *b = ((i * 73 + 19) & 0xFF) as u8;
    }

    for window_len in [1usize, 17, 64, 128] {
        let mut rolling = RollingWeakHash::new(&data[..window_len]);
        assert_eq!(
            rolling.sum32(),
            weak_hash(&data[..window_len]),
            "initial window mismatch for len={window_len}"
        );
        for start in 1..=(data.len() - window_len) {
            let out_byte = data[start - 1];
            let in_byte = data[start + window_len - 1];
            let rolled = rolling.roll(out_byte, in_byte);
            let fresh = weak_hash(&data[start..start + window_len]);
            assert_eq!(
                rolled, fresh,
                "rolling mismatch at offset={start}, len={window_len}"
            );
        }
    }
}

#[test]
fn test_bep_block_info_and_request_weak_hash_protobuf_roundtrip() {
    let block = BepBlockInfo {
        offset: 131_072,
        size: 131_072,
        hash: vec![0x42; 32],
        weak_hash: 0xDEAD_BEEF,
    };
    let index = BepIndex {
        folder: "shared".to_owned(),
        files: vec![BepFileInfo {
            name: "shifted.bin".to_owned(),
            is_dir: false,
            size: 262_144,
            modified_s: 1_700_000_000,
            deleted: false,
            invalid: false,
            sequence: 1,
            versions: vec![BepVersionCounter {
                short_device_id: 7,
                value: 1,
            }],
            blocks: vec![
                block.clone(),
                BepBlockInfo {
                    offset: 131_072,
                    size: 131_072,
                    hash: vec![0x11; 32],
                    weak_hash: 0,
                },
            ],
        }],
    };
    let decoded_index = BepIndex::decode(&index.encode()).expect("decode BepIndex");
    assert_eq!(decoded_index.files[0].blocks[0].weak_hash, 0xDEAD_BEEF);
    assert_eq!(decoded_index.files[0].blocks[1].weak_hash, 0);

    let req = BepRequest {
        id: 7,
        folder: "shared".to_owned(),
        name: "shifted.bin".to_owned(),
        offset: 0,
        size: 131_072,
        hash: vec![0x42; 32],
        weak_hash: 0x1234_5678,
    };
    let encoded = req.encode();
    let decoded_req = BepRequest::decode(&encoded).expect("decode BepRequest");
    assert_eq!(decoded_req, req);

    // When weak_hash == 0 it is omitted on the wire.
    let mut zero_req = req.clone();
    zero_req.weak_hash = 0;
    let zero_encoded = zero_req.encode();
    assert!(zero_encoded.len() < encoded.len());
    assert_eq!(BepRequest::decode(&zero_encoded).unwrap().weak_hash, 0);
}

#[test]
fn test_shifted_block_reuse_downloads_fewer_blocks_on_prepend() {
    let dir_a = common::fresh_temp_dir("shifted_peer_a");
    let dir_b = common::fresh_temp_dir("shifted_peer_b");
    let id_a = DeviceIdentity::from_seed(&[31u8; 32]).unwrap();
    let id_b = DeviceIdentity::from_seed(&[32u8; 32]).unwrap();

    let folder = "shifted";
    let root_a = dir_a.join("data");
    let root_b = dir_b.join("data");
    fs::create_dir_all(&root_a).unwrap();
    fs::create_dir_all(&root_b).unwrap();

    // 300 KiB deterministic non-repeating payload (3 blocks of 128 KiB, 128 KiB, 44 KiB).
    let base_len = 300 * 1024;
    let mut original = Vec::with_capacity(base_len);
    for i in 0..base_len {
        let x = ((i as u32).wrapping_mul(2654435761) >> 24) as u8;
        original.push(x ^ ((i >> 8) as u8));
    }

    let mut storage_a = FsSyncStorage::new(dir_a.join("db"));
    storage_a.set_folder_path(folder, &root_a);
    storage_a.set_folder_devices(folder, vec![id_b.device_id]);
    storage_a.add_local_folder(BepFolder {
        id: folder.to_owned(),
        label: folder.to_owned(),
        device_ids: vec![id_b.device_id],
    });

    let mut storage_b = FsSyncStorage::new(dir_b.join("db"));
    storage_b.set_folder_path(folder, &root_b);
    storage_b.set_folder_devices(folder, vec![id_a.device_id]);
    storage_b.add_local_folder(BepFolder {
        id: folder.to_owned(),
        label: folder.to_owned(),
        device_ids: vec![id_a.device_id],
    });

    // Helper: runs one A (listener) -> B (dialer) single-cycle sync and returns B's downloaded
    // block count.
    let run_sync_a_to_b = || -> usize {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let control_a = SyncControl::with_tuning(SyncTuning {
            rescan_interval: Duration::from_secs(60),
            keepalive_interval: Duration::from_secs(20),
        });
        let control_a_srv = Arc::clone(&control_a);
        let inbox_a = Arc::new(SecondaryInbox::new());

        let id_b_dev = id_b.device_id;
        let id_a_srv = id_a.clone();
        let storage_a_srv = storage_a.clone();
        let srv_thread = thread::spawn(move || {
            let (tcp, peer_addr) = listener.accept().unwrap();
            let incoming = accept_bep_tls_stream(&id_a_srv, tcp, TIMEOUT).unwrap();
            let hello = BepHello::new("peer-a", "test", "1");
            let params = SessionParams {
                identity: &id_a_srv,
                hello: &hello,
                storage: &storage_a_srv,
                expected_device_id: Some(id_b_dev),
                timeout: TIMEOUT,
                control: Some(control_a_srv),
                secondary_inbox: Some(inbox_a),
            };
            let _ = accept_and_sync(&params, incoming, peer_addr, |_| {});
        });

        let mut hello_b = BepHello::new("peer-b", "test", "1");
        hello_b.num_connections = 1;
        let params_b = SessionParams {
            identity: &id_b,
            hello: &hello_b,
            storage: &storage_b,
            expected_device_id: Some(id_a.device_id),
            timeout: TIMEOUT,
            control: None,
            secondary_inbox: None,
        };

        let downloaded_blocks = Arc::new(Mutex::new(0usize));
        let downloaded_blocks_ev = Arc::clone(&downloaded_blocks);
        let result_b = connect_and_sync(&params_b, addr, |ev| {
            if let SyncEvent::FileSynced {
                blocks_downloaded, ..
            } = ev
            {
                *downloaded_blocks_ev.lock().unwrap() += blocks_downloaded;
            }
        })
        .expect("sync session");

        control_a.request_stop();
        srv_thread.join().unwrap();
        assert!(matches!(
            result_b.session_status,
            SyncSessionStatus::Active { .. }
        ));
        let count = *downloaded_blocks.lock().unwrap();
        count
    };

    // Phase 1: create original 300 KiB file on A and sync it cleanly to B.
    fs::write(root_a.join("big.bin"), &original).unwrap();
    let initial_blocks = run_sync_a_to_b();
    assert_eq!(initial_blocks, 3);
    assert_eq!(fs::read(root_b.join("big.bin")).unwrap(), original);

    // Phase 2: prepend 17 bytes on A (shifting every 128 KiB block boundary!), then sync A -> B.
    let prefix = b"PREPENDED_17BYTES";
    assert_eq!(prefix.len(), 17);
    let mut shifted = Vec::with_capacity(prefix.len() + original.len());
    shifted.extend_from_slice(prefix);
    shifted.extend_from_slice(&original);
    fs::write(root_a.join("big.bin"), &shifted).unwrap();

    let total_blocks = shifted.len().div_ceil(BEP_BLOCK_SIZE);
    assert_eq!(total_blocks, 3);

    let blocks_fetched = run_sync_a_to_b();

    // Exact byte-for-byte file content on B.
    let synced_bytes = fs::read(root_b.join("big.bin")).unwrap();
    assert_eq!(
        Sha256::digest(&synced_bytes).as_slice(),
        Sha256::digest(&shifted).as_slice(),
        "synced file must match shifted source byte-for-byte"
    );

    // Block 0 (first 128 KiB with prepended 17 bytes) had to be downloaded;
    // Block 1 (128 KiB, shifted from 128 KiB - 17 in original) and Block 2 (tail) were reused locally!
    assert!(
        blocks_fetched < total_blocks,
        "expected fewer than {total_blocks} blocks downloaded via shifted weak_hash reuse, got {blocks_fetched}"
    );
    assert_eq!(
        blocks_fetched, 1,
        "only block 0 (containing the 17-byte prepend) should be fetched over the network"
    );

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn test_inbound_secondary_1_plus_2_attachment_and_sync() {
    let dir_srv = common::fresh_temp_dir("sec_inbox_srv");
    let dir_cli = common::fresh_temp_dir("sec_inbox_cli");
    let id_srv = DeviceIdentity::from_seed(&[41u8; 32]).unwrap();
    let id_cli = DeviceIdentity::from_seed(&[42u8; 32]).unwrap();

    let folder = "sec-shared";
    let root_srv = dir_srv.join("data");
    let root_cli = dir_cli.join("data");
    fs::create_dir_all(&root_srv).unwrap();
    fs::create_dir_all(&root_cli).unwrap();
    fs::write(root_cli.join("hello.txt"), b"via 1+2 inbound").unwrap();

    let mut storage_srv = FsSyncStorage::new(dir_srv.join("db"));
    storage_srv.set_folder_path(folder, &root_srv);
    storage_srv.set_folder_devices(folder, vec![id_cli.device_id]);
    storage_srv.add_local_folder(BepFolder {
        id: folder.to_owned(),
        label: folder.to_owned(),
        device_ids: vec![id_cli.device_id],
    });

    let mut storage_cli = FsSyncStorage::new(dir_cli.join("db"));
    storage_cli.set_folder_path(folder, &root_cli);
    storage_cli.set_folder_devices(folder, vec![id_srv.device_id]);
    storage_cli.add_local_folder(BepFolder {
        id: folder.to_owned(),
        label: folder.to_owned(),
        device_ids: vec![id_srv.device_id],
    });

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    let srv_control = SyncControl::with_tuning(SyncTuning {
        rescan_interval: Duration::from_secs(30),
        keepalive_interval: Duration::from_secs(20),
    });
    let srv_inbox = Arc::new(SecondaryInbox::new());
    let srv_claimed = Arc::new(AtomicBool::new(false));
    let srv_events = Arc::new(Mutex::new(Vec::<SyncEvent>::new()));

    let srv_control_loop = Arc::clone(&srv_control);
    let srv_inbox_loop = Arc::clone(&srv_inbox);
    let srv_claimed_loop = Arc::clone(&srv_claimed);
    let srv_events_loop = Arc::clone(&srv_events);
    let id_srv_loop = id_srv.clone();
    let id_cli_dev = id_cli.device_id;

    let srv_thread = thread::spawn(move || {
        let mut conn_threads = Vec::new();
        while !srv_control_loop.is_stop_requested() {
            match listener.accept() {
                Ok((tcp, peer_addr)) => {
                    tcp.set_nonblocking(false).unwrap();
                    let id = id_srv_loop.clone();
                    let storage = storage_srv.clone();
                    let control = Arc::clone(&srv_control_loop);
                    let inbox = Arc::clone(&srv_inbox_loop);
                    let claimed = Arc::clone(&srv_claimed_loop);
                    let events = Arc::clone(&srv_events_loop);
                    conn_threads.push(thread::spawn(move || {
                        let incoming = accept_bep_tls_stream(&id, tcp, TIMEOUT).unwrap();
                        assert_eq!(incoming.peer_device_id(), id_cli_dev);
                        let hello = BepHello::new("srv", "test", "1");
                        if !claimed.swap(true, Ordering::AcqRel) {
                            let params = SessionParams {
                                identity: &id,
                                hello: &hello,
                                storage: &storage,
                                expected_device_id: Some(id_cli_dev),
                                timeout: TIMEOUT,
                                control: Some(control),
                                secondary_inbox: Some(inbox),
                            };
                            let _ = accept_and_sync(&params, incoming, peer_addr, |ev| {
                                events.lock().unwrap().push(ev);
                            });
                        } else {
                            let mut tls = incoming.into_stream();
                            if accept_secondary_cluster_config(&mut tls, &hello).unwrap_or(false) {
                                let _ = inbox.deliver(&id_cli_dev, tls);
                            }
                        }
                    }));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("accept failed: {e}"),
            }
        }
        for h in conn_threads {
            let _ = h.join();
        }
    });

    let cli_control = SyncControl::with_tuning(SyncTuning {
        rescan_interval: Duration::from_secs(30),
        keepalive_interval: Duration::from_secs(20),
    });
    let cli_control_thread = Arc::clone(&cli_control);
    let cli_events = Arc::new(Mutex::new(Vec::<SyncEvent>::new()));
    let cli_events_thread = Arc::clone(&cli_events);
    let id_srv_dev = id_srv.device_id;

    let cli_thread = thread::spawn(move || {
        let mut hello = BepHello::new("cli", "test", "1");
        hello.num_connections = 3;
        let params = SessionParams {
            identity: &id_cli,
            hello: &hello,
            storage: &storage_cli,
            expected_device_id: Some(id_srv_dev),
            timeout: TIMEOUT,
            control: Some(cli_control_thread),
            secondary_inbox: None,
        };
        connect_and_sync(&params, addr, |ev| {
            cli_events_thread.lock().unwrap().push(ev);
        })
        .unwrap()
    });

    // Wait for both file sync and 1+2 attachment (active_connections == 3) on the server side.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_three = false;
    let mut synced_file = false;
    while Instant::now() < deadline {
        if Path::new(&root_srv.join("hello.txt"))
            .is_file()
            .then(|| fs::read(root_srv.join("hello.txt")).ok())
            .flatten()
            .as_deref()
            == Some(b"via 1+2 inbound")
        {
            synced_file = true;
        }
        if srv_events.lock().unwrap().iter().any(|ev| {
            matches!(
                ev,
                SyncEvent::SyncCycleCompleted {
                    active_connections: 3,
                    ..
                }
            )
        }) {
            saw_three = true;
        }
        if synced_file && saw_three {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    assert!(synced_file, "file did not sync over 1+2 inbound session");
    assert!(
        saw_three,
        "server did not attach both inbound secondaries (expected active_connections == 3, events: {:?})",
        *srv_events.lock().unwrap()
    );

    cli_control.request_stop();
    srv_control.request_stop();
    let cli_res = cli_thread.join().unwrap();
    srv_thread.join().unwrap();
    assert!(matches!(
        cli_res.session_status,
        SyncSessionStatus::Active { .. } | SyncSessionStatus::ClosedByPeer(_)
    ));

    let _ = fs::remove_dir_all(&dir_srv);
    let _ = fs::remove_dir_all(&dir_cli);
}
