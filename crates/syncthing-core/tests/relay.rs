//! Syncthing Relay Protocol v1 wire codec and session join integration tests.

mod common;

use std::io::{self, Cursor, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::thread;
use std::time::{Duration, Instant};

use common::tls_server::{spawn_tls_server, test_identity, ServerStream};
use syncthing_core::relay::client::join_relay_session;
use syncthing_core::relay::listen::RelayListener;
use syncthing_core::relay::pool::parse_relay_pool_json;
use syncthing_core::relay::wire::{
    decode_header, decode_payload, encode_connect_request, encode_join_relay_request,
    encode_join_session_request, encode_ping, encode_pong, read_relay_message, RelayMsgType,
    RELAY_ALPN, RELAY_MAGIC,
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

// ---------------------------------------------------------------------------
// Join request, keep-alive frames and generic message reading
// ---------------------------------------------------------------------------

#[test]
fn test_encode_join_relay_request_is_type_2_with_empty_token() {
    let frame = encode_join_relay_request();
    assert_eq!(
        frame,
        [0x9E, 0x79, 0xBC, 0x40, 0, 0, 0, 2, 0, 0, 0, 4, 0, 0, 0, 0]
    );
    let (msg_type, len) = decode_header(frame[..12].try_into().unwrap()).unwrap();
    assert_eq!(msg_type, RelayMsgType::JoinRelayRequest);
    assert_eq!(len, 4);
}

#[test]
fn test_encode_ping_and_pong_are_empty_frames() {
    let ping = encode_ping();
    let pong = encode_pong();
    assert_eq!(ping.len(), 12);
    assert_eq!(pong.len(), 12);
    assert_eq!(
        decode_header(ping[..].try_into().unwrap()).unwrap(),
        (RelayMsgType::Ping, 0)
    );
    assert_eq!(
        decode_header(pong[..].try_into().unwrap()).unwrap(),
        (RelayMsgType::Pong, 0)
    );
}

#[test]
fn test_read_relay_message_decodes_ping_and_pong() {
    assert_eq!(
        read_relay_message(&mut Cursor::new(encode_ping())).unwrap(),
        syncthing_core::RelayMessage::Ping
    );
    assert_eq!(
        read_relay_message(&mut Cursor::new(encode_pong())).unwrap(),
        syncthing_core::RelayMessage::Pong
    );
}

#[test]
fn test_read_relay_message_rejects_truncated_and_invalid_frames() {
    // Truncated header.
    assert!(read_relay_message(&mut Cursor::new(vec![0x9E, 0x79])).is_err());

    // Response announcing 8 payload bytes, only 2 present.
    let mut truncated = frame_relay_msg(RelayMsgType::Response, &[0; 8]);
    truncated.truncate(14);
    assert!(read_relay_message(&mut Cursor::new(truncated)).is_err());

    // Bad magic.
    let mut bad_magic = encode_pong();
    bad_magic[0] ^= 0xFF;
    assert!(read_relay_message(&mut Cursor::new(bad_magic)).is_err());

    // Unknown message type 99.
    let mut unknown_type = encode_ping();
    unknown_type[4..8].copy_from_slice(&99i32.to_be_bytes());
    assert!(read_relay_message(&mut Cursor::new(unknown_type)).is_err());

    // Payload length above the 65536-byte limit.
    let mut oversized = encode_ping();
    oversized[8..12].copy_from_slice(&65_537i32.to_be_bytes());
    assert!(read_relay_message(&mut Cursor::new(oversized)).is_err());
}

#[test]
fn test_join_relay_request_is_not_a_server_message() {
    // Only the client sends JoinRelayRequest: the relay answer decoder rejects it.
    assert!(decode_payload(RelayMsgType::JoinRelayRequest, &[0; 4]).is_err());
}

// ---------------------------------------------------------------------------
// Public relay pool
// ---------------------------------------------------------------------------

#[test]
fn test_parse_relay_pool_json_keeps_only_relay_urls() {
    let json = r#"{"relays":[{"url":"relay://1.2.3.4:22067/?id=AAAA&pingInterval=1m0s","location":{"latitude":1.5,"longitude":2.5,"city":"Paris"}},{"url":"https://example.org/not-a-relay"},{"url":"relay://[2001:db8::1]:22067/?id=BBBB","location":{}}]}"#;
    assert_eq!(
        parse_relay_pool_json(json),
        vec![
            "relay://1.2.3.4:22067/?id=AAAA&pingInterval=1m0s".to_owned(),
            "relay://[2001:db8::1]:22067/?id=BBBB".to_owned(),
        ]
    );
}

#[test]
fn test_parse_relay_pool_json_unescapes_slashes() {
    let json = r#"{"relays":[{"url":"relay:\/\/1.2.3.4:22067\/?id=AAAA"}]}"#;
    assert_eq!(
        parse_relay_pool_json(json),
        vec!["relay://1.2.3.4:22067/?id=AAAA".to_owned()]
    );
}

#[test]
fn test_parse_relay_pool_json_empty_or_invalid_is_empty() {
    assert!(parse_relay_pool_json(r#"{"relays":[]}"#).is_empty());
    assert!(parse_relay_pool_json("not json at all").is_empty());
}

// ---------------------------------------------------------------------------
// RelayListener (permanent join, invitations, keep-alive) against a fake relay
// ---------------------------------------------------------------------------

const LISTENER_TIMEOUT: Duration = Duration::from_secs(5);

/// Reads one frame without decoding its payload: the fake relay also receives the
/// client-only `JoinRelayRequest`, which `decode_payload` does not accept from a server.
fn read_raw_frame(stream: &mut ServerStream) -> io::Result<(RelayMsgType, Vec<u8>)> {
    let mut header = [0u8; 12];
    stream.read_exact(&mut header)?;
    let (msg_type, len) = decode_header(header)?;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    Ok((msg_type, payload))
}

fn send_frame(stream: &mut ServerStream, frame: &[u8]) -> io::Result<()> {
    stream.write_all(frame)?;
    stream.flush()
}

fn response_frame(code: i32, message: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&code.to_be_bytes());
    push_xdr_opaque(&mut payload, message.as_bytes());
    frame_relay_msg(RelayMsgType::Response, &payload)
}

fn invitation_frame(from: &[u8], key: &[u8], address: &[u8], port: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    push_xdr_opaque(&mut payload, from);
    push_xdr_opaque(&mut payload, key);
    push_xdr_opaque(&mut payload, address);
    payload.extend_from_slice(&port.to_be_bytes());
    payload.extend_from_slice(&0u32.to_be_bytes());
    frame_relay_msg(RelayMsgType::SessionInvitation, &payload)
}

fn relay_addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

#[test]
fn test_relay_listener_joins_answers_ping_and_receives_invitation() {
    let client = test_identity(0x31);
    let relay_identity = test_identity(0x32);
    let relay_id = relay_identity.device_id;
    let relay = spawn_tls_server(relay_identity, RELAY_ALPN, true, |stream| {
        let join = read_raw_frame(stream).expect("JoinRelayRequest");
        send_frame(stream, &response_frame(0, "success")).expect("Response");
        send_frame(stream, &frame_relay_msg(RelayMsgType::Ping, &[])).expect("Ping");
        send_frame(
            stream,
            &invitation_frame(&[0x11; 32], b"sess-42", &[198, 51, 100, 5], 22067),
        )
        .expect("SessionInvitation");
        let reply = read_raw_frame(stream).expect("Pong");
        (join, reply.0)
    });

    let addr = relay_addr(relay.port);
    let mut listener =
        RelayListener::join(&client, addr, relay_id, LISTENER_TIMEOUT).expect("join relay");
    assert_eq!(listener.relay_addr(), addr);
    assert_eq!(
        listener.relay_uri(),
        format!(
            "relay://127.0.0.1:{}/?id={}",
            relay.port,
            relay_id.to_syncthing_string()
        )
    );

    let invitation = listener
        .poll_invitation(LISTENER_TIMEOUT)
        .expect("poll")
        .expect("an invitation");
    assert_eq!(
        invitation,
        syncthing_core::SessionInvitation {
            from: vec![0x11; 32],
            key: b"sess-42".to_vec(),
            address: vec![198, 51, 100, 5],
            port: 22067,
            server_socket: false,
        }
    );

    drop(listener);
    let (((join_type, join_payload), pong_type), presented) = relay.join();
    assert_eq!(join_type, RelayMsgType::JoinRelayRequest);
    assert_eq!(join_payload, vec![0, 0, 0, 0]);
    assert_eq!(
        pong_type,
        RelayMsgType::Pong,
        "the Ping must be answered with a Pong"
    );
    assert_eq!(presented, Some(client.device_id));
}

#[test]
fn test_relay_listener_join_refused_by_relay_is_an_error() {
    let relay_identity = test_identity(0x33);
    let relay_id = relay_identity.device_id;
    let relay = spawn_tls_server(relay_identity, RELAY_ALPN, true, |stream| {
        let (msg_type, _) = read_raw_frame(stream).expect("JoinRelayRequest");
        send_frame(stream, &response_frame(1, "not allowed")).expect("Response");
        msg_type
    });
    let err = RelayListener::join(
        &test_identity(0x34),
        relay_addr(relay.port),
        relay_id,
        LISTENER_TIMEOUT,
    )
    .err()
    .expect("a refused join must fail");
    assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused);
    assert_eq!(relay.join().0, RelayMsgType::JoinRelayRequest);
}

#[test]
fn test_relay_listener_join_on_full_relay_is_an_error() {
    let relay_identity = test_identity(0x35);
    let relay_id = relay_identity.device_id;
    let relay = spawn_tls_server(relay_identity, RELAY_ALPN, true, |stream| {
        read_raw_frame(stream).expect("JoinRelayRequest");
        send_frame(stream, &frame_relay_msg(RelayMsgType::RelayFull, &[])).expect("RelayFull");
    });
    let err = RelayListener::join(
        &test_identity(0x36),
        relay_addr(relay.port),
        relay_id,
        LISTENER_TIMEOUT,
    )
    .err()
    .expect("a full relay must refuse the join");
    assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused);
    relay.join();
}

#[test]
fn test_relay_listener_rejects_relay_with_another_device_id() {
    let relay = spawn_tls_server(test_identity(0x37), RELAY_ALPN, true, |stream| {
        read_raw_frame(stream).is_ok()
    });
    let wrong_id = test_identity(0x38).device_id;
    let result = RelayListener::join(
        &test_identity(0x39),
        relay_addr(relay.port),
        wrong_id,
        LISTENER_TIMEOUT,
    );
    assert!(
        result.is_err(),
        "a relay that is not the pinned device must be refused"
    );
    let (reached, _) = relay.join();
    assert!(!reached, "the join request reached the relay");
}

#[test]
fn test_relay_listener_poll_without_invitation_returns_none() {
    let relay_identity = test_identity(0x3A);
    let relay_id = relay_identity.device_id;
    let relay = spawn_tls_server(relay_identity, RELAY_ALPN, true, |stream| {
        read_raw_frame(stream).expect("JoinRelayRequest");
        send_frame(stream, &response_frame(0, "success")).expect("Response");
        // Keeps the relay connected while the client polls.
        thread::sleep(Duration::from_millis(800));
    });
    let mut listener = RelayListener::join(
        &test_identity(0x3B),
        relay_addr(relay.port),
        relay_id,
        LISTENER_TIMEOUT,
    )
    .expect("join relay");

    let started = Instant::now();
    let outcome = listener
        .poll_invitation(Duration::from_millis(200))
        .expect("a silent relay is not an error");
    let elapsed = started.elapsed();

    assert!(outcome.is_none());
    assert!(
        elapsed >= Duration::from_millis(200),
        "returned before the poll time: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "poll did not return in time: {elapsed:?}"
    );
    drop(listener);
    relay.join();
}

#[test]
fn test_relay_listener_reports_a_closed_relay_connection() {
    let relay_identity = test_identity(0x3C);
    let relay_id = relay_identity.device_id;
    let relay = spawn_tls_server(relay_identity, RELAY_ALPN, true, |stream| {
        read_raw_frame(stream).expect("JoinRelayRequest");
        send_frame(stream, &response_frame(0, "success")).expect("Response");
        // The relay closes the connection when this returns.
    });
    let mut listener = RelayListener::join(
        &test_identity(0x3D),
        relay_addr(relay.port),
        relay_id,
        LISTENER_TIMEOUT,
    )
    .expect("join relay");
    relay.join();

    let result = listener.poll_invitation(LISTENER_TIMEOUT);
    assert!(
        result.is_err(),
        "a closed relay connection must be reported"
    );
}
