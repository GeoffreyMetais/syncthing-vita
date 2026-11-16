//! Session join of a relayed connection (`relay::client::join_relay_session`) against a fake
//! relay in session mode (plain TCP): only `Response(0, ...)` opens the tunnel.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use syncthing_core::relay::client::join_relay_session;
use syncthing_core::SessionInvitation;

const TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_MAGIC: u32 = 0x9E79_BC40;
const TYPE_RESPONSE: i32 = 4;

/// One complete relay frame: header, then `payload`.
fn frame(msg_type: i32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(&RELAY_MAGIC.to_be_bytes());
    out.extend_from_slice(&msg_type.to_be_bytes());
    out.extend_from_slice(&(payload.len() as i32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// A `Response` frame: code, then the message as an XDR opaque (padded).
fn response(code: i32, message: &str) -> Vec<u8> {
    let mut payload = code.to_be_bytes().to_vec();
    payload.extend_from_slice(&(message.len() as u32).to_be_bytes());
    payload.extend_from_slice(message.as_bytes());
    while payload.len() % 4 != 0 {
        payload.push(0);
    }
    frame(TYPE_RESPONSE, &payload)
}

/// A fake relay session endpoint: reads the `JoinSessionRequest`, answers `reply`, and returns
/// the request it received.
fn spawn_session_relay(reply: Vec<u8>) -> (SocketAddr, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind relay");
    let addr = listener.local_addr().expect("address");
    let handle = thread::spawn(move || {
        let (mut tcp, _) = listener.accept().expect("accept");
        let mut header = [0u8; 12];
        tcp.read_exact(&mut header).expect("request header");
        let len = i32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
        let mut request = header.to_vec();
        request.resize(12 + len, 0);
        tcp.read_exact(&mut request[12..]).expect("request payload");
        tcp.write_all(&reply).expect("reply");
        tcp.flush().expect("flush");
        request
    });
    (addr, handle)
}

fn invitation(port: u16) -> SessionInvitation {
    SessionInvitation {
        from: vec![0x11; 32],
        key: vec![1, 2, 3, 4],
        address: vec![127, 0, 0, 1],
        port,
        server_socket: false,
    }
}

#[test]
fn a_successful_join_opens_the_tunnel_and_sends_the_session_key() {
    let (addr, relay) = spawn_session_relay(response(0, "success"));
    let tunnel = join_relay_session(addr, &invitation(addr.port()), TIMEOUT);
    let request = relay.join().expect("relay");
    assert!(tunnel.is_ok(), "{:?}", tunnel.err());
    // Type 3 (JoinSessionRequest), then the key 1, 2, 3, 4 as an XDR opaque.
    assert_eq!(&request[4..8], &3i32.to_be_bytes());
    assert_eq!(&request[12..], &[0, 0, 0, 4, 1, 2, 3, 4]);
}

#[test]
fn a_join_refused_with_a_non_zero_code_is_an_error() {
    let (addr, relay) = spawn_session_relay(response(1, "wrong key"));
    let err =
        join_relay_session(addr, &invitation(addr.port()), TIMEOUT).expect_err("refused join");
    relay.join().expect("relay");
    assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused, "{err}");
    assert!(err.to_string().contains("(1)"), "{err}");
}

#[test]
fn a_full_relay_refuses_the_join() {
    let (addr, relay) = spawn_session_relay(frame(7, &[]));
    let err = join_relay_session(addr, &invitation(addr.port()), TIMEOUT).expect_err("full relay");
    relay.join().expect("relay");
    assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused, "{err}");
    assert!(err.to_string().contains("full"), "{err}");
}
