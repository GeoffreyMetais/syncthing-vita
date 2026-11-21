//! Integration tests for Milestone P2.5:
//! - Pure-Rust NAT traversal (NAT-PMP RFC 6886 & UPnP IGD SSDP/SOAP codecs + loopback UDP)
//! - `.stversions` file versioning (`TrashCan`, `Simple` pruning, `.stversions` scan exclusion)
//! - `FolderType::ReceiveEncrypted` wire encoding/decoding, key derivation, block encryption, and
//!   outbound local-change guard.

use std::fs;
use std::net::{Ipv4Addr, UdpSocket};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use syncthing_core::bep::{
    BepClusterConfig, BepFileInfo, BepFolder, BepIndex, BepVersionCounter, FolderIndexState,
    FolderType,
};
use syncthing_core::discovery::nat::{
    build_add_port_mapping_soap, build_get_external_ip_soap, build_ssdp_msearch,
    decode_natpmp_map_resp, decode_natpmp_public_addr_resp, encode_natpmp_map_req,
    encode_natpmp_public_addr_req, natpmp_map_tcp_at, parse_external_ip_soap,
    parse_igd_control_url, parse_ssdp_location, NatProtocol,
};
use syncthing_core::folder_sync::{mask_local_changes, IgnoreMatcher};
use syncthing_core::storage::encrypted::{
    decrypt_block, derive_folder_key, derive_metadata_key, encrypt_block, encrypted_rel_path,
    suppresses_local_outbound_changes, BLOCK_NONCE_LEN,
};
use syncthing_core::storage::names::{is_internal_name, is_syncable_name};
use syncthing_core::storage::versioning::{archive_file_at, format_versioned_filename};
use syncthing_core::{DeviceId, FileVersioning, FsSyncStorage};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "syncthing_p2_5_{name}_{}_{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn natpmp_wire_codecs_and_loopback_gateway() {
    let pub_req = encode_natpmp_public_addr_req();
    assert_eq!(pub_req, [0, 0]);

    let mut pub_resp = [0u8; 12];
    pub_resp[0] = 0;
    pub_resp[1] = 0x80;
    pub_resp[2..4].copy_from_slice(&0u16.to_be_bytes());
    pub_resp[4..8].copy_from_slice(&12345u32.to_be_bytes());
    pub_resp[8..12].copy_from_slice(&[203, 0, 113, 42]);
    let decoded_pub = decode_natpmp_public_addr_resp(&pub_resp).expect("decode public addr");
    assert_eq!(decoded_pub.result_code, 0);
    assert_eq!(decoded_pub.epoch_secs, 12345);
    assert_eq!(decoded_pub.external_ip, Ipv4Addr::new(203, 0, 113, 42));

    let map_req = encode_natpmp_map_req(22000, 22001, 3600);
    assert_eq!(map_req[0], 0);
    assert_eq!(map_req[1], 2);
    assert_eq!(u16::from_be_bytes([map_req[4], map_req[5]]), 22000);
    assert_eq!(u16::from_be_bytes([map_req[6], map_req[7]]), 22001);
    assert_eq!(
        u32::from_be_bytes([map_req[8], map_req[9], map_req[10], map_req[11]]),
        3600
    );

    let mut map_resp = [0u8; 16];
    map_resp[0] = 0;
    map_resp[1] = 0x82;
    map_resp[4..8].copy_from_slice(&12346u32.to_be_bytes());
    map_resp[8..10].copy_from_slice(&22000u16.to_be_bytes());
    map_resp[10..12].copy_from_slice(&22001u16.to_be_bytes());
    map_resp[12..16].copy_from_slice(&3600u32.to_be_bytes());
    let decoded_map = decode_natpmp_map_resp(&map_resp).expect("decode map resp");
    assert_eq!(decoded_map.internal_port, 22000);
    assert_eq!(decoded_map.external_port, 22001);
    assert_eq!(decoded_map.lifetime_secs, 3600);

    // Live loopback UDP mock NAT-PMP gateway
    let server = UdpSocket::bind("127.0.0.1:0").expect("bind mock gateway");
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let server_addr = server.local_addr().expect("gateway addr");

    let handle = thread::spawn(move || {
        let mut buf = [0u8; 64];
        // Request 1: public address
        let (n1, peer1) = server.recv_from(&mut buf).expect("recv req 1");
        assert_eq!(&buf[..n1], &[0, 0]);
        let mut r1 = [0u8; 12];
        r1[0] = 0;
        r1[1] = 0x80;
        r1[4..8].copy_from_slice(&777u32.to_be_bytes());
        r1[8..12].copy_from_slice(&[198, 51, 100, 99]);
        server.send_to(&r1, peer1).expect("send resp 1");

        // Request 2: TCP mapping
        let (n2, peer2) = server.recv_from(&mut buf).expect("recv req 2");
        assert_eq!(n2, 12);
        assert_eq!(buf[1], 2);
        let int_p = u16::from_be_bytes([buf[4], buf[5]]);
        let ext_p = u16::from_be_bytes([buf[6], buf[7]]);
        let life = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let mut r2 = [0u8; 16];
        r2[0] = 0;
        r2[1] = 0x82;
        r2[4..8].copy_from_slice(&778u32.to_be_bytes());
        r2[8..10].copy_from_slice(&int_p.to_be_bytes());
        r2[10..12].copy_from_slice(&ext_p.to_be_bytes());
        r2[12..16].copy_from_slice(&life.to_be_bytes());
        server.send_to(&r2, peer2).expect("send resp 2");
    });

    let mapping = natpmp_map_tcp_at(server_addr, 22000, 22050, 7200, Duration::from_secs(2))
        .expect("loopback natpmp map");
    handle.join().expect("mock gateway thread");

    assert_eq!(mapping.external_ip, Ipv4Addr::new(198, 51, 100, 99));
    assert_eq!(mapping.external_port, 22050);
    assert_eq!(mapping.lease_secs, 7200);
    assert_eq!(mapping.protocol, NatProtocol::NatPmp);
}

#[test]
fn upnp_ssdp_igd_xml_and_soap_parsing() {
    let msearch = build_ssdp_msearch();
    assert!(msearch.starts_with("M-SEARCH * HTTP/1.1\r\n"));
    assert!(msearch.contains("HOST: 239.255.255.250:1900\r\n"));
    assert!(msearch.contains("ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n"));

    let ssdp_resp = "HTTP/1.1 200 OK\r\n\
                     CACHE-CONTROL: max-age=120\r\n\
                     LoCaTiOn: http://192.168.1.1:5000/rootDesc.xml\r\n\
                     ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n";
    assert_eq!(
        parse_ssdp_location(ssdp_resp).as_deref(),
        Some("http://192.168.1.1:5000/rootDesc.xml")
    );

    let igd_xml = r#"<?xml version="1.0"?>
        <root xmlns="urn:schemas-upnp-org:device-1-0">
          <device>
            <serviceList>
              <service>
                <serviceType>urn:schemas-upnp-org:service:Layer3Forwarding:1</serviceType>
                <controlURL>/ctl/L3F</controlURL>
              </service>
              <service>
                <serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>
                <controlURL>/ctl/IPConn</controlURL>
              </service>
            </serviceList>
          </device>
        </root>"#;
    let (st, ctrl) = parse_igd_control_url(igd_xml).expect("parse controlURL");
    assert_eq!(st, "urn:schemas-upnp-org:service:WANIPConnection:1");
    assert_eq!(ctrl, "/ctl/IPConn");

    let add_soap = build_add_port_mapping_soap(
        &st,
        22000,
        22000,
        Ipv4Addr::new(192, 168, 1, 50),
        3600,
        "syncthing-vita",
    );
    assert!(add_soap
        .contains("<u:AddPortMapping xmlns:u=\"urn:schemas-upnp-org:service:WANIPConnection:1\">"));
    assert!(add_soap.contains("<NewExternalPort>22000</NewExternalPort>"));
    assert!(add_soap.contains("<NewInternalClient>192.168.1.50</NewInternalClient>"));
    assert!(add_soap.contains("<NewLeaseDuration>3600</NewLeaseDuration>"));

    let ip_req = build_get_external_ip_soap(&st);
    assert!(ip_req.contains(
        "<u:GetExternalIPAddress xmlns:u=\"urn:schemas-upnp-org:service:WANIPConnection:1\"/>"
    ));

    let ip_resp = r#"<?xml version="1.0"?>
        <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
          <s:Body>
            <u:GetExternalIPAddressResponse xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
              <NewExternalIPAddress>203.0.113.7</NewExternalIPAddress>
            </u:GetExternalIPAddressResponse>
          </s:Body>
        </s:Envelope>"#;
    assert_eq!(
        parse_external_ip_soap(ip_resp),
        Some(Ipv4Addr::new(203, 0, 113, 7))
    );
}

#[test]
fn file_versioning_trashcan_simple_pruning_and_scan_exclusion() {
    let base = temp_dir("versioning");
    let db_root = base.join("db");
    let folder_root = base.join("folder");
    fs::create_dir_all(&db_root).unwrap();
    fs::create_dir_all(&folder_root).unwrap();

    // 1. TrashCan archives both on overwrite (commit_download) and remote deletion (plan_pull).
    let mut storage = FsSyncStorage::new(&db_root);
    storage.set_folder_path("ver-trash", &folder_root);
    storage.set_folder_versioning("ver-trash", FileVersioning::TrashCan { cleanout_days: 0 });

    let sub_dir = folder_root.join("saves");
    fs::create_dir_all(&sub_dir).unwrap();
    let target = sub_dir.join("slot1.bin");
    fs::write(&target, b"old-save-v1").unwrap();

    storage
        .prepare_download_seeded("ver-trash", "saves/slot1.bin", false)
        .unwrap();
    storage
        .write_block_streaming("ver-trash", "saves/slot1.bin", 0, b"new-save-v2", 11)
        .unwrap();
    storage
        .commit_download("ver-trash", "saves/slot1.bin", 11)
        .unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"new-save-v2");
    assert_eq!(
        fs::read(folder_root.join(".stversions/saves/slot1.bin")).unwrap(),
        b"old-save-v1"
    );

    // Remote deletion moves current version to .stversions too.
    let own_id = DeviceId([1u8; 32]);
    let (local_idx, _) = storage
        .build_local_index("ver-trash", own_id, &[])
        .expect("build index");
    // Ensure `.stversions` is never indexed and IgnoreMatcher excludes it.
    assert!(is_internal_name(".stversions"));
    assert!(!is_syncable_name(".stversions/saves/slot1.bin"));
    assert!(IgnoreMatcher::empty().is_ignored(".stversions/saves/slot1.bin"));
    assert!(local_idx
        .files
        .iter()
        .all(|f| !f.name.contains(".stversions")));

    let existing_entry = local_idx
        .files
        .iter()
        .find(|f| f.name == "saves/slot1.bin")
        .expect("slot1.bin indexed");
    let mut deleted_remote = existing_entry.clone();
    deleted_remote.deleted = true;
    deleted_remote.size = 0;
    deleted_remote.blocks.clear();
    deleted_remote.versions = vec![
        existing_entry.versions[0],
        BepVersionCounter {
            short_device_id: 0xDEAD_BEEF,
            value: 99,
        },
    ];
    let plan = storage
        .plan_pull("ver-trash", own_id, &[deleted_remote])
        .expect("plan pull deletion");
    assert!(plan
        .applied
        .iter()
        .any(|m| m.contains("Deleted saves/slot1.bin")));
    assert!(!target.exists());
    assert_eq!(
        fs::read(folder_root.join(".stversions/saves/slot1.bin")).unwrap(),
        b"new-save-v2"
    );

    // 2. Simple versioning archives with `~YYYYMMDD-HHMMSS` and prunes oldest beyond `keep = 2`.
    let simple_root = base.join("simple");
    fs::create_dir_all(&simple_root).unwrap();
    let f = simple_root.join("note.txt");
    let t1 = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let t2 = UNIX_EPOCH + Duration::from_secs(1_700_000_100);
    let t3 = UNIX_EPOCH + Duration::from_secs(1_700_000_200);

    for (ts, body) in [(t1, b"v1"), (t2, b"v2"), (t3, b"v3")] {
        fs::write(&f, body).unwrap();
        archive_file_at(
            &simple_root,
            "note.txt",
            &f,
            FileVersioning::Simple { keep: 2 },
            ts,
        )
        .expect("archive simple");
    }

    let n1 = format_versioned_filename("note.txt", t1);
    let n2 = format_versioned_filename("note.txt", t2);
    let n3 = format_versioned_filename("note.txt", t3);
    assert!(!simple_root.join(".stversions").join(&n1).exists());
    assert_eq!(
        fs::read(simple_root.join(".stversions").join(&n2)).unwrap(),
        b"v2"
    );
    assert_eq!(
        fs::read(simple_root.join(".stversions").join(&n3)).unwrap(),
        b"v3"
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn receive_encrypted_wire_crypto_and_outbound_guard() {
    assert_eq!(FolderType::ReceiveEncrypted.wire_value(), 3);
    assert_eq!(FolderType::from_wire(3), FolderType::ReceiveEncrypted);

    let own_id = DeviceId([0x11; 32]);
    let peer_id = DeviceId([0x22; 32]);
    let folder = BepFolder {
        id: "enc-folder".to_owned(),
        label: "Encrypted".to_owned(),
        folder_type: FolderType::ReceiveEncrypted,
        device_ids: vec![peer_id],
        ..Default::default()
    };
    let state = FolderIndexState {
        index_id: 42,
        max_sequence: 7,
        paused: false,
        folder_type: FolderType::ReceiveEncrypted,
    };
    let encoded =
        BepClusterConfig::encode_for_peers(&[(folder.clone(), state)], own_id, "local", peer_id);
    let decoded = BepClusterConfig::decode(&encoded).expect("decode ClusterConfig");
    assert_eq!(decoded.folders.len(), 1);
    assert_eq!(decoded.folders[0].folder_type, FolderType::ReceiveEncrypted);

    // Direct encode() also preserves FolderType::ReceiveEncrypted
    let direct_bytes = BepClusterConfig {
        folders: vec![folder],
        secondary: false,
    }
    .encode();
    let direct_decoded =
        BepClusterConfig::decode(&direct_bytes).expect("decode direct ClusterConfig");
    assert_eq!(
        direct_decoded.folders[0].folder_type,
        FolderType::ReceiveEncrypted
    );

    // Storage helper & key derivation + XChaCha20-Poly1305 round-trip
    let base = temp_dir("enc_storage");
    let mut storage = FsSyncStorage::new(&base);
    storage.set_folder_type("enc-folder", FolderType::ReceiveEncrypted);
    assert!(storage.is_folder_receive_encrypted("enc-folder"));
    assert!(!storage.is_folder_receive_encrypted("other-folder"));
    assert!(suppresses_local_outbound_changes(
        storage.folder_type("enc-folder")
    ));

    let key_a = derive_folder_key("enc-folder", "correct horse battery staple");
    let key_b = derive_folder_key("enc-folder", "correct horse battery staple");
    let key_other = derive_folder_key("other-folder", "correct horse battery staple");
    assert_eq!(key_a, key_b);
    assert_ne!(key_a, key_other);
    assert_ne!(derive_metadata_key(&key_a), key_a);

    let path_a = encrypted_rel_path(&key_a, "docs/secret.txt");
    let path_b = encrypted_rel_path(&key_a, "docs/secret.txt");
    assert_eq!(path_a, path_b);
    assert!(path_a.contains(".syncthing-enc/"));

    let nonce = [7u8; BLOCK_NONCE_LEN];
    let plaintext = b"encrypted block payload on untrusted peer";
    let sealed = encrypt_block(&key_a, &nonce, plaintext).expect("encrypt block");
    assert_ne!(&sealed[BLOCK_NONCE_LEN..], plaintext);
    let opened = decrypt_block(&key_a, &sealed).expect("decrypt block");
    assert_eq!(opened, plaintext);
    assert!(decrypt_block(&key_other, &sealed).is_err());

    // Guard behavior: local changes in ReceiveEncrypted are masked `invalid` before outbound send.
    let local_index = BepIndex {
        folder: "enc-folder".to_owned(),
        files: vec![BepFileInfo {
            name: "rogue-local.txt".to_owned(),
            size: 5,
            versions: vec![BepVersionCounter {
                short_device_id: 1,
                value: 1,
            }],
            blocks: vec![syncthing_core::BepBlockInfo {
                offset: 0,
                size: 5,
                hash: Sha256::digest(b"hello").to_vec(),
                weak_hash: 0,
            }],
            ..Default::default()
        }],
    };
    let masked = mask_local_changes(&local_index, &[]);
    assert!(masked.files[0].invalid);

    let _ = fs::remove_dir_all(&base);
}
