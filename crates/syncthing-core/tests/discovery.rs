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

#[test]
fn test_parse_global_discovery_json_extracts_addresses() {
    let body = r#"{
        "seen": "2026-11-14T12:00:00Z",
        "addresses": [
            "tcp://203.0.113.10:22000",
            "relay://198.51.100.5:22067/?id=7LONO6G-3333333-4444444-5555555-6666666-7777777-8888888-9999999&pingInterval=1m0s",
            "quic://203.0.113.10:22000"
        ]
    }"#;
    let uris = syncthing_core::parse_global_discovery_json(body);
    assert_eq!(uris.len(), 3);
    assert_eq!(uris[0], "tcp://203.0.113.10:22000");
    assert!(uris[1].starts_with("relay://198.51.100.5:22067/?id="));
    assert_eq!(uris[2], "quic://203.0.113.10:22000");
}

#[test]
fn test_parse_endpoint_uri_supports_tcp_and_relay_and_filters_unspecified() {
    use syncthing_core::{parse_endpoint_uri, PeerEndpoint};

    let relay_id = DeviceId([0xAB; 32]);
    let relay_uri = format!(
        "relay://198.51.100.5:22067/?id={}&pingInterval=1m0s",
        relay_id.to_syncthing_string()
    );

    assert_eq!(
        parse_endpoint_uri("tcp://203.0.113.10:22000"),
        Some(PeerEndpoint::Direct("203.0.113.10:22000".parse().unwrap()))
    );
    assert_eq!(
        parse_endpoint_uri(&relay_uri),
        Some(PeerEndpoint::Relay {
            addr: "198.51.100.5:22067".parse().unwrap(),
            device_id: relay_id,
        })
    );
    assert_eq!(parse_endpoint_uri("tcp://0.0.0.0:22000"), None);
    assert_eq!(parse_endpoint_uri("quic://203.0.113.10:22000"), None);
    assert_eq!(parse_endpoint_uri("relay://198.51.100.5:22067/"), None);
}

