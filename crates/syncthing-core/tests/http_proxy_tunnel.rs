//! `CONNECT` tunnel responses of `tls::proxy::HttpProxy`: the response head size limit, a
//! proxy that closes without answering, and the host / port split of unbracketed IPv6 hosts.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use syncthing_core::HttpProxy;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Reads the `CONNECT` request head of `client` (up to the blank line).
fn read_connect_head(client: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if client.read(&mut byte).expect("CONNECT head") == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

/// A proxy that reads the `CONNECT` head, then writes `reply` as its answer and closes.
fn spawn_proxy_replying(reply: Vec<u8>) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind proxy");
    let port = listener.local_addr().expect("address").port();
    let handle = thread::spawn(move || {
        let (mut client, _) = listener.accept().expect("accept");
        let _ = read_connect_head(&mut client);
        let _ = client.write_all(&reply);
        let _ = client.flush();
    });
    (port, handle)
}

fn proxy_at(port: u16) -> HttpProxy {
    HttpProxy::parse(&format!("http://127.0.0.1:{port}")).expect("valid proxy URL")
}

#[test]
fn a_proxy_answer_head_under_16_kib_is_accepted() {
    // A 4 KiB head (padding header) is well within the limit. The tunnel target is not
    // contacted by this fake proxy, so any port will do.
    let padding = "a".repeat(4000);
    let reply = format!("HTTP/1.1 200 OK\r\nX-Pad: {padding}\r\n\r\n").into_bytes();
    let (proxy_port, proxy) = spawn_proxy_replying(reply);
    let tunnel = proxy_at(proxy_port).connect("127.0.0.1", 9, TIMEOUT);
    proxy.join().expect("proxy");
    assert!(tunnel.is_ok(), "{:?}", tunnel.err());
}

#[test]
fn a_proxy_answer_head_over_16_kib_is_refused() {
    let (proxy_port, proxy) = spawn_proxy_replying(vec![b'a'; 20 * 1024]);
    let err = proxy_at(proxy_port)
        .connect("127.0.0.1", 9, TIMEOUT)
        .expect_err("oversized answer head");
    proxy.join().expect("proxy");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
}

#[test]
fn a_proxy_closing_without_an_answer_is_an_invalid_response() {
    // Nothing is written: the proxy closes right after reading the request.
    let (proxy_port, proxy) = spawn_proxy_replying(Vec::new());
    let err = proxy_at(proxy_port)
        .connect("127.0.0.1", 9, TIMEOUT)
        .expect_err("no answer");
    proxy.join().expect("proxy");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
}

#[test]
fn an_unbracketed_ipv6_proxy_host_without_port_defaults_to_80() {
    let proxy = HttpProxy::parse("http://::1").expect("valid proxy URL");
    assert_eq!(proxy.host, "::1");
    assert_eq!(proxy.port, 80);
}
