//! HTTP proxy tunnels (`tls::proxy::HttpProxy`) for the HTTPS requests to infrastructure
//! servers: URL parsing, `CONNECT` with credentials, refusals, and a full request through a
//! local proxy to a local pinned HTTPS server.

mod common;

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use common::tls_server::{
    spawn_tls_server, test_identity, try_read_http_request, write_http_response,
};
use syncthing_core::discovery::http::https_request;
use syncthing_core::{HttpProxy, HttpsClient, HttpsServer};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A one-connection proxy: records the `CONNECT` header, answers `status`, and when it is
/// `200` pipes the tunnel to `127.0.0.1:<target_port>`.
fn spawn_proxy(status: &'static str, target_port: u16) -> (u16, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let (mut client, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") && client.read(&mut byte).unwrap() == 1 {
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        client
            .write_all(format!("HTTP/1.1 {status}\r\n\r\n").as_bytes())
            .unwrap();
        if status.starts_with("200") {
            let upstream = TcpStream::connect(("127.0.0.1", target_port)).unwrap();
            pipe(client, upstream);
        }
        head
    });
    (port, handle)
}

/// Copies bytes both ways until either side closes.
fn pipe(a: TcpStream, b: TcpStream) {
    let (mut a_read, mut b_write) = (a.try_clone().unwrap(), b.try_clone().unwrap());
    let forward = thread::spawn(move || {
        let _ = io::copy(&mut a_read, &mut b_write);
        let _ = b_write.shutdown(std::net::Shutdown::Write);
    });
    let (mut b_read, mut a_write) = (b, a);
    let _ = io::copy(&mut b_read, &mut a_write);
    let _ = a_write.shutdown(std::net::Shutdown::Write);
    let _ = forward.join();
}

#[test]
fn proxy_urls_are_parsed() {
    let p = HttpProxy::parse("http://127.0.0.1:3128").unwrap();
    assert_eq!((p.host.as_str(), p.port), ("127.0.0.1", 3128));
    let p = HttpProxy::parse("proxy.example.org").unwrap();
    assert_eq!((p.host.as_str(), p.port), ("proxy.example.org", 80));
    let p = HttpProxy::parse("http://user:secret@proxy.example.org:8080/").unwrap();
    assert_eq!((p.host.as_str(), p.port), ("proxy.example.org", 8080));
    let p = HttpProxy::parse("http://[::1]:3128").unwrap();
    assert_eq!((p.host.as_str(), p.port), ("::1", 3128));
    assert!(HttpProxy::parse("socks5://127.0.0.1:1080").is_none());
    assert!(HttpProxy::parse("http://:3128").is_none());
    assert!(HttpProxy::parse("http://host:notaport").is_none());
}

#[test]
fn https_request_goes_through_the_proxy_tunnel() {
    let server = spawn_tls_server(test_identity(0x61), b"http/1.1", false, |stream| {
        let request = try_read_http_request(stream).expect("request");
        write_http_response(stream, "200 OK", &[], "{\"addresses\":[]}").expect("response");
        request
    });
    let server_port = server.port;
    let (proxy_port, proxy) = spawn_proxy("200 Connection established", server_port);
    let url = format!(
        "https://localhost:{}/v2/?id={}",
        server.port,
        server.device_id.to_syncthing_string()
    );
    let target = HttpsServer::parse(&url).unwrap();
    let client = HttpsClient {
        proxy: HttpProxy::parse(&format!("http://alice:s3cret@127.0.0.1:{proxy_port}")),
        extra_roots: Vec::new(),
    };
    let response = https_request(
        &target,
        &client,
        "GET",
        "/v2/?device=X",
        None,
        None,
        TIMEOUT,
    )
    .expect("request through the proxy");
    let (request, _) = server.join();
    let connect = proxy.join().unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(request.request_line(), "GET /v2/?device=X HTTP/1.1");
    assert!(
        connect.starts_with(&format!("CONNECT localhost:{server_port} HTTP/1.1\r\n")),
        "{connect}"
    );
    // `alice:s3cret` in Base64.
    assert!(
        connect.contains("Proxy-Authorization: Basic YWxpY2U6czNjcmV0\r\n"),
        "{connect}"
    );
}

#[test]
fn a_refused_connect_is_an_error_naming_the_status() {
    let (proxy_port, proxy) = spawn_proxy("403 Forbidden", 1);
    let target = HttpsServer::parse("https://blocked.example.org/").unwrap();
    let client = HttpsClient {
        proxy: HttpProxy::parse(&format!("http://127.0.0.1:{proxy_port}")),
        extra_roots: Vec::new(),
    };
    let err = https_request(&target, &client, "GET", "/", None, None, TIMEOUT).unwrap_err();
    let connect = proxy.join().unwrap();
    assert!(err.to_string().contains("403"), "{err}");
    assert!(!connect.contains("Proxy-Authorization"));
}

#[test]
fn pem_roots_are_added_and_garbage_is_ignored() {
    let der = test_identity(0x62).cert_der;
    let pem = format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\nnot a certificate\n",
        base64(&der)
    );
    let mut client = HttpsClient::default();
    assert_eq!(client.add_pem_roots(pem.as_bytes()), 1);
    assert_eq!(client.extra_roots[0].as_ref(), der.as_slice());
    assert_eq!(client.add_pem_roots(b"nothing here"), 0);
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[((n >> (18 - 6 * i)) & 0x3F) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}
