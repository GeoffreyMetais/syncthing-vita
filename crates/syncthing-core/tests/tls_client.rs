//! TLS 1.3 client side (`tls::client`, `tls::BepTlsStream`): the BEP ID probe, ALPN
//! negotiation of outgoing streams, and the buffered writes of a wrapped stream.

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use common::tls_server::{spawn_tls_server, test_identity};
use syncthing_core::tls::client::{open_bep_tls_stream, open_tls_stream_with_alpn};
use syncthing_core::tls::probe_bep_peer_device_id;
use syncthing_core::BEP_ALPN;

const TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn probe_returns_the_device_id_of_a_bep_peer() {
    let server = spawn_tls_server(test_identity(0x81), BEP_ALPN, false, |stream| {
        // Stays up until the probe closes its side.
        let mut byte = [0u8; 1];
        let _ = stream.read(&mut byte);
    });
    let addr = format!("127.0.0.1:{}", server.port)
        .parse()
        .expect("address");
    let probed = probe_bep_peer_device_id(addr, TIMEOUT);
    server.join();
    assert_eq!(probed, Some(test_identity(0x81).device_id));
}

#[test]
fn probe_of_a_plain_http_server_finds_no_device_id() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("address");
    let server = thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        let _ = tcp.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
    });
    assert_eq!(probe_bep_peer_device_id(addr, TIMEOUT), None);
    server.join().expect("plain server");
}

#[test]
fn outgoing_streams_negotiate_the_requested_alpn() {
    let server = spawn_tls_server(test_identity(0x82), b"bep-relay", false, |stream| {
        // The first read drives the server handshake and returns the client's byte.
        let mut byte = [0u8; 1];
        let _ = stream.read(&mut byte);
        stream.conn.alpn_protocol().map(<[u8]>::to_vec)
    });
    let addr = format!("127.0.0.1:{}", server.port)
        .parse()
        .expect("address");
    let (mut stream, _) =
        open_tls_stream_with_alpn(&test_identity(0x83), addr, b"bep-relay", None, TIMEOUT)
            .expect("TLS stream");
    // The client handshake starts with the first write.
    stream.write_all(b"x").expect("write");
    stream.flush().expect("flush");
    drop(stream);
    let (negotiated, _) = server.join();
    assert_eq!(negotiated.as_deref(), Some(&b"bep-relay"[..]));
}

#[test]
fn a_bep_stream_is_a_client_stream_and_flushes_its_writes() {
    let server = spawn_tls_server(test_identity(0x84), BEP_ALPN, false, |stream| {
        let mut received = [0u8; 5];
        // The client's bytes arrive as a whole TLS record: the read completes.
        stream.read_exact(&mut received).map(|()| received)
    });
    let addr = format!("127.0.0.1:{}", server.port)
        .parse()
        .expect("address");
    let (mut stream, _) =
        open_bep_tls_stream(&test_identity(0x85), addr, None, TIMEOUT).expect("BEP stream");
    assert!(!stream.is_server(), "an outgoing stream is a client stream");
    stream.write_all(b"hello").expect("write");
    stream.flush().expect("flush");
    let (received, _) = server.join();
    assert_eq!(
        received.expect("server received the flushed bytes"),
        *b"hello"
    );
}

#[test]
fn a_flush_reports_output_the_stalled_peer_never_took() {
    // The server completes the handshake, then stops reading: the socket buffers fill up and
    // a write leaves TLS output pending (its timeout error is ignored by `write`).
    let server = spawn_tls_server(test_identity(0x86), BEP_ALPN, false, |stream| {
        let mut byte = [0u8; 1];
        let _ = stream.read(&mut byte);
        thread::sleep(Duration::from_secs(1));
    });
    let addr = format!("127.0.0.1:{}", server.port)
        .parse()
        .expect("address");
    let short_timeout = Duration::from_millis(300);
    let (mut stream, _) =
        open_bep_tls_stream(&test_identity(0x87), addr, None, short_timeout).expect("BEP stream");
    stream.write_all(b"x").expect("first byte drives the handshake");

    let chunk = vec![0u8; 256 * 1024];
    let mut stalled = false;
    for _ in 0..256 {
        if stream.write(&chunk).is_err() {
            stalled = true;
            break;
        }
    }
    assert!(stalled, "the stalled peer never blocked the writes");
    assert!(
        stream.flush().is_err(),
        "flush must report the output the peer never took"
    );
    drop(stream);
    server.join();
}
