//! Pre-authentication `Hello` frames, post-`Hello` message framing and the BEP size constants.

use std::io::{Cursor, ErrorKind, Read, Write};

use syncthing_core::{
    read_bep_message, write_bep_message, BepHello, BepMessageType, BEP_BLOCK_SIZE, BEP_MAGIC,
    BEP_MAX_BLOCK_SIZE,
};

const MAGIC: [u8; 4] = [0x2E, 0xA7, 0xD9, 0x0B];

#[test]
fn block_size_constants_are_exact() {
    assert_eq!(BEP_BLOCK_SIZE, 128 * 1024);
    assert_eq!(BEP_BLOCK_SIZE, 131_072);
    assert_eq!(BEP_MAX_BLOCK_SIZE, 16 * 1024 * 1024);
    assert_eq!(BEP_MAX_BLOCK_SIZE, 16_777_216);
    assert_eq!(BEP_MAGIC, 0x2EA7_D90B);
}

#[test]
fn hello_frame_has_exact_bytes() {
    let hello = BepHello::new("d", "c", "v");
    let mut expected = MAGIC.to_vec();
    expected.extend_from_slice(&[0x00, 0x0B]); // payload length 11
    expected.extend_from_slice(&[0x0A, 0x01, b'd']); // device_name
    expected.extend_from_slice(&[0x12, 0x01, b'c']); // client_name
    expected.extend_from_slice(&[0x1A, 0x01, b'v']); // client_version
    expected.extend_from_slice(&[0x20, 0x03]); // num_connections = 3
    assert_eq!(hello.encode_frame(), expected);
}

#[test]
fn hello_frame_omits_empty_strings_zero_connections_and_timestamp() {
    let hello = BepHello {
        device_name: String::new(),
        client_name: String::new(),
        client_version: String::new(),
        num_connections: 0,
        timestamp: 0,
    };
    assert_eq!(hello.encode_frame(), [MAGIC.to_vec(), vec![0, 0]].concat());
}

#[test]
fn hello_frame_writes_timestamp_only_when_positive() {
    let mut hello = BepHello::new("", "", "");
    hello.num_connections = 0;
    hello.timestamp = 5;
    assert_eq!(
        hello.encode_frame(),
        [MAGIC.to_vec(), vec![0x00, 0x02, 0x28, 0x05]].concat()
    );
    hello.timestamp = -1;
    assert_eq!(hello.encode_frame(), [MAGIC.to_vec(), vec![0, 0]].concat());
}

#[test]
fn hello_frame_roundtrip_and_fields() {
    let mut hello = BepHello::new("PS-Vita", "vita-syncthing", "0.1.0");
    hello.timestamp = 1_700_000_000_000_000_001;
    let decoded = BepHello::decode_frame(&hello.encode_frame()).unwrap();
    assert_eq!(decoded, hello);
    assert_eq!(decoded.num_connections, 3);
}

#[test]
fn hello_decode_rejects_bad_magic_short_and_truncated_frames() {
    let hello = BepHello::new("d", "c", "v").encode_frame();

    let mut bad_magic = hello.clone();
    bad_magic[0] ^= 0xFF;
    assert_eq!(
        BepHello::decode_frame(&bad_magic).unwrap_err().kind(),
        ErrorKind::InvalidData
    );

    assert_eq!(
        BepHello::decode_frame(&hello[..5]).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );

    // Declared payload of 11 bytes, only 5 present: must be an error, not a panic.
    assert_eq!(
        BepHello::decode_frame(&hello[..11]).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
    // Exactly the declared length decodes.
    assert!(BepHello::decode_frame(&hello).is_ok());
}

#[test]
fn hello_decode_payload_ignores_unknown_fields() {
    use syncthing_core::proto::encode_varint_field;
    let mut payload = Vec::new();
    encode_varint_field(&mut payload, 9, 1);
    encode_varint_field(&mut payload, 4, 2);
    let hello = BepHello::decode_payload(&payload).unwrap();
    assert_eq!(hello.num_connections, 2);
    assert_eq!(hello.device_name, "");
}

/// Duplex stream: reads a canned peer reply and records everything written by the local side.
struct Duplex {
    incoming: Cursor<Vec<u8>>,
    outgoing: Vec<u8>,
}

impl Read for Duplex {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.incoming.read(buf)
    }
}

impl Write for Duplex {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.outgoing.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn hello_exchange_sends_a_positive_nanosecond_timestamp() {
    let peer = BepHello::new("peer", "x", "1").encode_frame();
    let mut stream = Duplex {
        incoming: Cursor::new(peer),
        outgoing: Vec::new(),
    };
    let reply = BepHello::new("PS-Vita", "vita", "0.1")
        .exchange(&mut stream)
        .unwrap();
    assert_eq!(reply.device_name, "peer");

    let sent = BepHello::decode_frame(&stream.outgoing).unwrap();
    assert_eq!(sent.device_name, "PS-Vita");
    // A Unix time in nanoseconds: far above 2020-01-01 (1.577e18 ns).
    assert!(
        sent.timestamp > 1_577_000_000_000_000_000,
        "timestamp {} is not a nanosecond clock",
        sent.timestamp
    );
}

#[test]
fn hello_exchange_rejects_bad_peer_magic() {
    let mut peer = BepHello::new("peer", "x", "1").encode_frame();
    peer[3] ^= 0x01;
    let mut stream = Duplex {
        incoming: Cursor::new(peer),
        outgoing: Vec::new(),
    };
    let err = BepHello::new("a", "b", "c")
        .exchange(&mut stream)
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn write_message_emits_exact_frame_layout() {
    let mut out = Vec::new();
    write_bep_message(&mut out, BepMessageType::Ping, &[1, 2]).unwrap();
    // header length 2, header Ping (field 1 = 6), payload length 2, payload.
    assert_eq!(
        out,
        vec![0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x02, 1, 2]
    );

    let mut out = Vec::new();
    write_bep_message(&mut out, BepMessageType::ClusterConfig, &[]).unwrap();
    assert_eq!(out, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn read_message_roundtrips_written_frames() {
    let mut out = Vec::new();
    write_bep_message(&mut out, BepMessageType::Index, b"payload").unwrap();
    let (kind, payload) = read_bep_message(&mut Cursor::new(out)).unwrap();
    assert_eq!(kind, BepMessageType::Index);
    assert_eq!(payload, b"payload");
}

#[test]
fn read_message_decompresses_lz4_frames() {
    // Header: type Ping (field 1 = 6), compression LZ4 (field 2 = 1).
    let lz4_payload = [0, 0, 0, 2, 0x20, b'h', b'i'];
    let mut frame = vec![0x00, 0x04, 0x08, 0x06, 0x10, 0x01];
    frame.extend_from_slice(&(lz4_payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&lz4_payload);
    let (kind, payload) = read_bep_message(&mut Cursor::new(frame)).unwrap();
    assert_eq!(kind, BepMessageType::Ping);
    assert_eq!(payload, b"hi");
}

/// Builds a frame whose declared message length is `msg_len`, with `body` bytes after it.
fn frame_with_length(msg_len: u32, body: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x00, 0x02, 0x08, 0x06];
    frame.extend_from_slice(&msg_len.to_be_bytes());
    frame.extend_from_slice(body);
    frame
}

#[test]
fn read_message_rejects_frames_just_above_the_limit() {
    let limit: u32 = 16 * 1024 * 1024;
    let over = frame_with_length(limit + 1, &[]);
    let err = read_bep_message(&mut Cursor::new(over)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn read_message_accepts_a_frame_of_exactly_the_limit() {
    let limit: u32 = 16 * 1024 * 1024;
    let body = vec![7u8; limit as usize];
    let frame = frame_with_length(limit, &body);
    let (kind, payload) = read_bep_message(&mut Cursor::new(frame)).unwrap();
    assert_eq!(kind, BepMessageType::Ping);
    assert_eq!(payload.len(), limit as usize);
}

#[test]
fn read_message_reports_truncated_payload_as_eof() {
    let frame = frame_with_length(10, &[1, 2, 3]);
    let err = read_bep_message(&mut Cursor::new(frame)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
}
