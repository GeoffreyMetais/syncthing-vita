//! Syncthing Relay Protocol v1 wire codec and session join integration tests.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener};
use std::thread;
use std::time::Duration;

use syncthing_core::relay::client::join_relay_session;
use syncthing_core::relay::wire::{
    decode_header, decode_payload, encode_connect_request, encode_join_session_request,
    RelayMsgType, RELAY_MAGIC,
};
use syncthing_core::{DeviceId, RelayMessage, RelayResponse, SessionInvitation};

fn push_xdr_opaque(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(data);
    let pad = (4 - (data.len() % 4)) % 4;
    buf.extend(std::iter::repeat_n(0u8, pad));
}

fn frame_relay_msg(msg_type: RelayMsgType, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(&RELAY_MAGIC.to_be_bytes());
    out.extend_from_slice(&(msg_type as i32).to_be_bytes());
    out.extend_from_slice(&(payload.len() as i32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

#[test]
fn test_encode_connect_and_join_requests_have_valid_xdr_headers() {
    let target = DeviceId([0x5A; 32]);
    let connect = encode_connect_request(&target);
    let (msg_type, len) = decode_header(connect[..12].try_into().unwrap()).unwrap();
    assert_eq!(msg_type, RelayMsgType::ConnectRequest);
    assert_eq!(len, 36);
    assert_eq!(&connect[12..16], &32u32.to_be_bytes());
    assert_eq!(&connect[16..48], &target.0);

    let key = b"secret-key";
    let join = encode_join_session_request(key);
    let (msg_type, len) = decode_header(join[..12].try_into().unwrap()).unwrap();
    assert_eq!(msg_type, RelayMsgType::JoinSessionRequest);
    // 4-byte length + 10-byte key + 2-byte XDR padding = 16
    assert_eq!(len, 16);
    assert_eq!(join.len(), 12 + 16);
    assert_eq!(&join[26..28], &[0, 0]);
}

#[test]
fn test_decode_header_rejects_bad_magic_and_negative_length() {
    let bad_magic = [0u8; 12];
    assert!(decode_header(bad_magic).is_err());

    let mut bad_len = Vec::new();
    bad_len.extend_from_slice(&RELAY_MAGIC.to_be_bytes());
    bad_len.extend_from_slice(&(RelayMsgType::Pong as i32).to_be_bytes());
    bad_len.extend_from_slice(&(-1i32).to_be_bytes());
    assert!(decode_header(bad_len.try_into().unwrap()).is_err());
}

#[test]
fn test_decode_payload_session_invitation_and_response() {
    let mut inv_payload = Vec::new();
    push_xdr_opaque(&mut inv_payload, &[0x11; 32]);
    push_xdr_opaque(&mut inv_payload, b"sess1");
    push_xdr_opaque(&mut inv_payload, &[198, 51, 100, 5]);
    inv_payload.extend_from_slice(&22067u32.to_be_bytes());
    inv_payload.extend_from_slice(&0u32.to_be_bytes());

    let msg = decode_payload(RelayMsgType::SessionInvitation, &inv_payload).unwrap();
    assert_eq!(
        msg,
        RelayMessage::SessionInvitation(SessionInvitation {
            from: vec![0x11; 32],
            key: b"sess1".to_vec(),
            address: vec![198, 51, 100, 5],
            port: 22067,
            server_socket: false,
        })
    );

    let mut resp_payload = Vec::new();
    resp_payload.extend_from_slice(&0i32.to_be_bytes());
    push_xdr_opaque(&mut resp_payload, b"success");
    let resp = decode_payload(RelayMsgType::Response, &resp_payload).unwrap();
    assert_eq!(
        resp,
        RelayMessage::Response(RelayResponse {
            code: 0,
            message: "success".to_owned(),
        })
    );

    assert_eq!(
        decode_payload(RelayMsgType::RelayFull, &[]).unwrap(),
        RelayMessage::RelayFull
    );
}

#[test]
fn test_session_invitation_ip_fallback() {
    let fallback = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 5));
    let make_inv = |address: Vec<u8>| SessionInvitation {
        from: Vec::new(),
        key: Vec::new(),
        address,
        port: 22067,
        server_socket: false,
    };

    assert_eq!(make_inv(vec![]).session_ip(fallback), fallback);
    assert_eq!(make_inv(vec![0, 0, 0, 0]).session_ip(fallback), fallback);
    assert_eq!(make_inv(vec![0; 16]).session_ip(fallback), fallback);
    assert_eq!(
        make_inv(vec![203, 0, 113, 9]).session_ip(fallback),
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9))
    );
    assert_eq!(
        make_inv(Ipv6Addr::LOCALHOST.octets().to_vec()).session_ip(fallback),
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    );
}

#[test]
fn test_join_relay_session_over_loopback_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut hdr = [0u8; 12];
        stream.read_exact(&mut hdr).unwrap();
        let (msg_type, len) = decode_header(hdr).unwrap();
        assert_eq!(msg_type, RelayMsgType::JoinSessionRequest);
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).unwrap();
        assert_eq!(&payload[4..12], b"test-key");

        let mut resp = Vec::new();
        resp.extend_from_slice(&0i32.to_be_bytes());
        push_xdr_opaque(&mut resp, b"success");
        stream
            .write_all(&frame_relay_msg(RelayMsgType::Response, &resp))
            .unwrap();
        stream.write_all(b"TUNNELED").unwrap();
    });

    let inv = SessionInvitation {
        from: vec![0x22; 32],
        key: b"test-key".to_vec(),
        address: vec![0, 0, 0, 0],
        port: addr.port(),
        server_socket: false,
    };

    let mut tunnel = join_relay_session(addr, &inv, Duration::from_secs(2)).unwrap();
    let mut buf = [0u8; 8];
    tunnel.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"TUNNELED");
    server.join().unwrap();
}

/// An opaque length larger than the payload (up to `u32::MAX`, which wraps a 32-bit `usize`
/// addition) is a truncated message, never a panic.
#[test]
fn test_decode_payload_rejects_oversized_opaque_length() {
    for len in [5u32, u32::MAX - 2, u32::MAX] {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_be_bytes()); // Response code
        payload.extend_from_slice(&len.to_be_bytes()); // message length
        payload.extend_from_slice(b"ok");
        assert!(decode_payload(RelayMsgType::Response, &payload).is_err());
    }
}
