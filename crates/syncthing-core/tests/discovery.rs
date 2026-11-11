//! Local Discovery v4: `Announce` codec and advertised address resolution.

use std::net::SocketAddr;

use syncthing_core::{resolve_peer_tcp_addrs, Announce, DeviceId};

#[test]
fn test_announce_protobuf_roundtrip() {
    let id = DeviceId([0x42; 32]);
    let original = Announce {
        id,
        addresses: vec![
            "tcp://192.168.1.55:22000".to_owned(),
            "quic://192.168.1.55:22000".to_owned(),
        ],
        instance_id: 0x1234_5678_9ABC_DEF0,
    };

    let encoded = original.encode_packet();
    assert_eq!(&encoded[0..4], &[0x2E, 0xA7, 0xD9, 0x0B]);

    let decoded = Announce::decode_packet(&encoded).expect("Announce decoding failed");
    assert_eq!(decoded, original);
}

#[test]
fn test_resolve_peer_tcp_addrs_prefers_udp_source_ip() {
    let udp_src: SocketAddr = "192.168.1.20:21027".parse().unwrap();
    let addrs = resolve_peer_tcp_addrs(
        udp_src,
        &[
            "tcp://10.0.0.5:22000".to_owned(),
            "quic://192.168.1.20:22000".to_owned(),
            "tcp://0.0.0.0:22000".to_owned(),
            "tcp://192.168.1.20:22000".to_owned(),
        ],
    );
    let expected: Vec<SocketAddr> = vec![
        "192.168.1.20:22000".parse().unwrap(),
        "10.0.0.5:22000".parse().unwrap(),
    ];
    assert_eq!(addrs, expected);
}
