//! Global Discovery v2: endpoint URI parsing, lookups and announcements against a local
//! pinned HTTPS server that checks the request and presents its own certificate.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use common::tls_server::{
    spawn_tls_server, test_identity, try_read_http_request, write_http_response,
};
use syncthing_core::discovery::global::{
    lookup_peer_on, parse_global_discovery_json, resolve_endpoint_uri, PeerEndpoint,
};
use syncthing_core::discovery::global_announce::{
    announce_on, encode_announcement, AnnounceOutcome, DEFAULT_ANNOUNCE_RETRY,
    DEFAULT_REANNOUNCE_INTERVAL,
};
use syncthing_core::tls::https::HttpsServer;
use syncthing_core::DeviceId;
use syncthing_core::HttpsClient;

const TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_SEED: u8 = 0x21;
const PEER_SEED: u8 = 0x22;
const RELAY_SEED: u8 = 0x23;
const CLIENT_SEED: u8 = 0x24;

/// `https://127.0.0.1:<port>/v2/?<flags>&id=<server ID>`: a pinned discovery server URL.
fn pinned_server_url(port: u16, server_id: DeviceId, flags: &str) -> String {
    format!(
        "https://127.0.0.1:{port}/v2/?{flags}&id={}",
        server_id.to_syncthing_string()
    )
}

#[test]
fn encode_announcement_escapes_quotes_and_backslashes() {
    let addresses = vec![
        r#"tcp://a"b\c:22000"#.to_owned(),
        "relay://h:1/?id=X".to_owned(),
    ];
    assert_eq!(
        encode_announcement(&addresses),
        r#"{"addresses":["tcp://a\"b\\c:22000","relay://h:1/?id=X"]}"#
    );
}

#[test]
fn encode_announcement_without_addresses() {
    assert_eq!(encode_announcement(&[]), r#"{"addresses":[]}"#);
}

#[test]
fn resolve_endpoint_uri_accepts_direct_and_relay_addresses() {
    assert_eq!(
        resolve_endpoint_uri("tcp://127.0.0.1:22000"),
        Some(PeerEndpoint::Direct(
            "127.0.0.1:22000".parse::<SocketAddr>().unwrap()
        ))
    );
    assert_eq!(
        resolve_endpoint_uri("tcp://[::1]:22000"),
        Some(PeerEndpoint::Direct(
            "[::1]:22000".parse::<SocketAddr>().unwrap()
        ))
    );
    let relay_id = DeviceId([0x55; 32]);
    let uri = format!(
        "relay://127.0.0.1:22067/?id={}&pingInterval=1m0s",
        relay_id.to_syncthing_string()
    );
    assert_eq!(
        resolve_endpoint_uri(&uri),
        Some(PeerEndpoint::Relay {
            addr: "127.0.0.1:22067".parse().unwrap(),
            device_id: relay_id,
        })
    );
}

#[test]
fn resolve_endpoint_uri_rejects_unusable_addresses() {
    // Unspecified addresses, unknown schemes, relays without an ID and malformed ports.
    assert_eq!(resolve_endpoint_uri("tcp://0.0.0.0:22000"), None);
    assert_eq!(resolve_endpoint_uri("quic://127.0.0.1:22000"), None);
    assert_eq!(resolve_endpoint_uri("relay://127.0.0.1:22067/"), None);
    assert_eq!(resolve_endpoint_uri("tcp://127.0.0.1:notaport"), None);
    assert_eq!(resolve_endpoint_uri("127.0.0.1:22000"), None);
}

#[test]
fn parse_global_discovery_json_lists_every_uri() {
    let json = r#"{"addresses":["tcp://127.0.0.1:22000","tcp://[::1]:22000"]}"#;
    assert_eq!(
        parse_global_discovery_json(json),
        vec![
            "tcp://127.0.0.1:22000".to_owned(),
            "tcp://[::1]:22000".to_owned()
        ]
    );
}

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

#[test]
fn lookup_returns_the_usable_endpoints_of_the_peer() {
    let peer = test_identity(PEER_SEED);
    let relay = test_identity(RELAY_SEED);
    let body = format!(
        r#"{{"addresses":["tcp://127.0.0.1:22000","tcp://0.0.0.0:22000","tcp://127.0.0.1:22000","relay://127.0.0.1:22067/?id={}&pingInterval=1m0s","quic://127.0.0.1:22001"]}}"#,
        relay.device_id.to_syncthing_string()
    );
    let server = spawn_tls_server(
        test_identity(SERVER_SEED),
        b"http/1.1",
        false,
        move |stream| {
            let request = try_read_http_request(stream).expect("lookup request");
            write_http_response(
                stream,
                "200 OK",
                &[("Content-Type", "application/json")],
                &body,
            )
            .expect("lookup response");
            request
        },
    );
    let target = HttpsServer::parse(&pinned_server_url(
        server.port,
        server.device_id,
        "nolookup",
    ))
    .expect("valid URL");

    let endpoints =
        lookup_peer_on(&target, &HttpsClient::default(), peer.device_id, TIMEOUT).expect("lookup");
    let (request, _) = server.join();

    assert_eq!(
        request.request_line(),
        format!(
            "GET /v2/?device={} HTTP/1.1",
            peer.device_id.to_syncthing_string()
        )
    );
    assert_eq!(
        endpoints,
        vec![
            PeerEndpoint::Direct("127.0.0.1:22000".parse().unwrap()),
            PeerEndpoint::Relay {
                addr: "127.0.0.1:22067".parse().unwrap(),
                device_id: relay.device_id,
            },
        ]
    );
}

#[test]
fn lookup_of_an_unknown_device_is_empty() {
    let server = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", false, |stream| {
        let request = try_read_http_request(stream).expect("lookup request");
        write_http_response(stream, "404 Not Found", &[], "").expect("response");
        request
    });
    let target = HttpsServer::parse(&pinned_server_url(
        server.port,
        server.device_id,
        "nolookup",
    ))
    .expect("valid URL");
    let endpoints = lookup_peer_on(
        &target,
        &HttpsClient::default(),
        test_identity(PEER_SEED).device_id,
        TIMEOUT,
    )
    .expect("404 is not an error");
    server.join();
    assert!(endpoints.is_empty());
}

#[test]
fn lookup_server_error_is_an_error() {
    let server = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", false, |stream| {
        try_read_http_request(stream).expect("lookup request");
        write_http_response(stream, "500 Internal Server Error", &[], "oops").expect("response");
    });
    let target = HttpsServer::parse(&pinned_server_url(
        server.port,
        server.device_id,
        "nolookup",
    ))
    .expect("valid URL");
    let err = lookup_peer_on(
        &target,
        &HttpsClient::default(),
        test_identity(PEER_SEED).device_id,
        TIMEOUT,
    )
    .expect_err("HTTP 500 must fail");
    assert!(err.to_string().contains("500"), "unexpected error: {err}");
    server.join();
}

// ---------------------------------------------------------------------------
// Announcement
// ---------------------------------------------------------------------------

#[test]
fn announce_presents_our_certificate_and_honours_reannounce_after() {
    let client = test_identity(CLIENT_SEED);
    let server = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", true, |stream| {
        let request = try_read_http_request(stream).expect("announcement");
        write_http_response(
            stream,
            "204 No Content",
            &[("Reannounce-After", "1800")],
            "",
        )
        .expect("response");
        request
    });
    let target = HttpsServer::parse(&pinned_server_url(
        server.port,
        server.device_id,
        "nolookup",
    ))
    .expect("valid URL");
    let addresses = vec![
        "tcp://203.0.113.10:22000".to_owned(),
        "relay://198.51.100.5:22067/?id=ABC".to_owned(),
    ];

    let outcome = announce_on(
        &target,
        &HttpsClient::default(),
        &client,
        &addresses,
        TIMEOUT,
    )
    .expect("announce");
    let (request, presented) = server.join();

    assert_eq!(
        outcome,
        AnnounceOutcome::Accepted {
            reannounce_after: Duration::from_secs(1800)
        }
    );
    assert_eq!(request.request_line(), "POST /v2/ HTTP/1.1");
    assert!(request.head.contains("Content-Type: application/json"));
    assert_eq!(request.body, encode_announcement(&addresses));
    assert_eq!(
        presented,
        Some(client.device_id),
        "the server must learn our DeviceId"
    );
}

#[test]
fn announce_refused_with_retry_after() {
    let server = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", true, |stream| {
        try_read_http_request(stream).expect("announcement");
        write_http_response(
            stream,
            "429 Too Many Requests",
            &[("Retry-After", "60")],
            "",
        )
        .expect("response");
    });
    let target = HttpsServer::parse(&pinned_server_url(
        server.port,
        server.device_id,
        "noannounce",
    ))
    .expect("valid URL");
    let outcome = announce_on(
        &target,
        &HttpsClient::default(),
        &test_identity(CLIENT_SEED),
        &[],
        TIMEOUT,
    )
    .expect("announce");
    server.join();
    assert_eq!(
        outcome,
        AnnounceOutcome::Refused {
            status: 429,
            retry_after: Duration::from_secs(60),
        }
    );
}

#[test]
fn announce_refusal_falls_back_to_reannounce_after() {
    let server = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", true, |stream| {
        try_read_http_request(stream).expect("announcement");
        write_http_response(
            stream,
            "503 Service Unavailable",
            &[("Reannounce-After", "120")],
            "",
        )
        .expect("response");
    });
    let target = HttpsServer::parse(&pinned_server_url(
        server.port,
        server.device_id,
        "nolookup",
    ))
    .expect("valid URL");
    let outcome = announce_on(
        &target,
        &HttpsClient::default(),
        &test_identity(CLIENT_SEED),
        &[],
        TIMEOUT,
    )
    .expect("announce");
    server.join();
    assert_eq!(
        outcome,
        AnnounceOutcome::Refused {
            status: 503,
            retry_after: Duration::from_secs(120),
        }
    );
}

#[test]
fn announce_without_headers_uses_the_defaults() {
    let accepted = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", true, |stream| {
        try_read_http_request(stream).expect("announcement");
        write_http_response(stream, "204 No Content", &[], "").expect("response");
    });
    let target = HttpsServer::parse(&pinned_server_url(
        accepted.port,
        accepted.device_id,
        "nolookup",
    ))
    .expect("valid URL");
    let outcome = announce_on(
        &target,
        &HttpsClient::default(),
        &test_identity(CLIENT_SEED),
        &[],
        TIMEOUT,
    )
    .expect("announce");
    accepted.join();
    assert_eq!(
        outcome,
        AnnounceOutcome::Accepted {
            reannounce_after: DEFAULT_REANNOUNCE_INTERVAL
        }
    );

    let refused = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", true, |stream| {
        try_read_http_request(stream).expect("announcement");
        write_http_response(stream, "429 Too Many Requests", &[], "").expect("response");
    });
    let target = HttpsServer::parse(&pinned_server_url(
        refused.port,
        refused.device_id,
        "nolookup",
    ))
    .expect("valid URL");
    let outcome = announce_on(
        &target,
        &HttpsClient::default(),
        &test_identity(CLIENT_SEED),
        &[],
        TIMEOUT,
    )
    .expect("announce");
    refused.join();
    assert_eq!(
        outcome,
        AnnounceOutcome::Refused {
            status: 429,
            retry_after: DEFAULT_ANNOUNCE_RETRY,
        }
    );
}

#[test]
fn announce_without_any_response_is_an_error() {
    let server = spawn_tls_server(test_identity(SERVER_SEED), b"http/1.1", true, |stream| {
        // Reads the announcement, then closes the connection without answering.
        try_read_http_request(stream).expect("announcement");
    });
    let target = HttpsServer::parse(&pinned_server_url(
        server.port,
        server.device_id,
        "nolookup",
    ))
    .expect("valid URL");
    let result = announce_on(
        &target,
        &HttpsClient::default(),
        &test_identity(CLIENT_SEED),
        &[],
        TIMEOUT,
    );
    server.join();
    assert!(result.is_err());
}
