//! HTTP/1.1 exchanges over pinned HTTPS (`discovery::http::https_request`): response size
//! limit, read timeouts, the chunked-body guard of `parse_http_response`, and the
//! `host[:port]` authority split of `HttpsServer::parse` for unbracketed IPv6 hosts.

mod common;

use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use common::tls_server::{
    spawn_tls_server, test_identity, try_read_http_request, write_http_response,
};
use syncthing_core::discovery::http::{https_request, parse_http_response, HttpResponse};
use syncthing_core::{DeviceId, HttpsClient, HttpsServer};

/// Read timeout of the client in the timeout tests: short, so the tests stay fast.
const SHORT_TIMEOUT: Duration = Duration::from_millis(300);
/// How long a server stays silent after its last write in the timeout tests (longer than
/// [`SHORT_TIMEOUT`]).
const SILENCE: Duration = Duration::from_millis(1500);
/// Size limit of a response (`discovery::http::MAX_RESPONSE_BYTES`).
const ONE_MIB: usize = 1024 * 1024;

fn pinned_server(port: u16, id: DeviceId) -> HttpsServer {
    let url = format!(
        "https://127.0.0.1:{port}/v2/?nolookup&id={}",
        id.to_syncthing_string()
    );
    HttpsServer::parse(&url).expect("valid URL")
}

fn get(server: &HttpsServer, timeout: Duration) -> io::Result<HttpResponse> {
    https_request(
        server,
        &HttpsClient::default(),
        "GET",
        "/v2/",
        None,
        None,
        timeout,
    )
}

#[test]
fn a_body_of_several_kilobytes_is_read_whole() {
    let body = "x".repeat(10_000);
    let expected = body.clone();
    let server = spawn_tls_server(test_identity(0x71), b"http/1.1", false, move |stream| {
        try_read_http_request(stream).expect("request");
        write_http_response(stream, "200 OK", &[], &body).expect("response");
    });
    let target = pinned_server(server.port, server.device_id);
    let response = get(&target, Duration::from_secs(5)).expect("response");
    server.join();
    assert_eq!(response.status, 200);
    assert_eq!(response.body.len(), 10_000);
    assert_eq!(response.body, expected);
}

#[test]
fn a_response_larger_than_one_mebibyte_is_cut_at_the_limit() {
    let body = "y".repeat(3 * ONE_MIB / 2);
    let server = spawn_tls_server(test_identity(0x72), b"http/1.1", false, move |stream| {
        try_read_http_request(stream).expect("request");
        // The client stops reading at the limit and closes: the write may fail.
        let _ = write_http_response(stream, "200 OK", &[], &body);
    });
    let target = pinned_server(server.port, server.device_id);
    let response = get(&target, Duration::from_secs(5)).expect("truncated response");
    server.join();
    // The reader stops once the raw response reaches the limit: the body keeps almost all
    // of it (the head takes a few dozen bytes, a read adds at most one 4 KiB buffer).
    assert!(
        response.body.len() >= ONE_MIB - 1024,
        "body cut too early: {}",
        response.body.len()
    );
    assert!(
        response.body.len() < ONE_MIB + 4096,
        "body not cut at the limit: {}",
        response.body.len()
    );
}

#[test]
fn a_silent_server_is_a_timeout_not_an_invalid_response() {
    let server = spawn_tls_server(test_identity(0x73), b"http/1.1", false, |stream| {
        try_read_http_request(stream).expect("request");
        thread::sleep(SILENCE);
    });
    let target = pinned_server(server.port, server.device_id);
    let err = get(&target, SHORT_TIMEOUT).expect_err("no response, so an error");
    server.join();
    assert!(
        matches!(
            err.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ),
        "expected a timeout, got {err:?}"
    );
}

#[test]
fn a_complete_response_is_kept_when_the_server_then_stays_silent() {
    let server = spawn_tls_server(test_identity(0x74), b"http/1.1", false, |stream| {
        try_read_http_request(stream).expect("request");
        // A complete answer, but no `close_notify`: the connection just stays open.
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .expect("response");
        stream.flush().expect("flush");
        thread::sleep(SILENCE);
    });
    let target = pinned_server(server.port, server.device_id);
    let response = get(&target, SHORT_TIMEOUT).expect("the complete response is returned");
    server.join();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, "ok");
}

#[test]
fn only_a_chunked_transfer_coding_is_decoded() {
    // A header named differently but valued `chunked` does not make the body chunked.
    let parsed = parse_http_response("HTTP/1.1 200 OK\r\nX-Mode: chunked\r\n\r\nhello")
        .expect("well-formed response");
    assert_eq!(parsed.body, "hello");

    // Another transfer coding is not decoded as chunked either.
    let parsed = parse_http_response("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nhello")
        .expect("well-formed response");
    assert_eq!(parsed.body, "hello");
}

#[test]
fn an_unbracketed_ipv6_host_without_port_defaults_to_443() {
    let server = HttpsServer::parse("https://2001:db8::5/v2/").expect("valid URL");
    assert_eq!(server.host, "2001:db8::5");
    assert_eq!(server.port, 443);
    assert_eq!(server.path, "/v2/");
}
