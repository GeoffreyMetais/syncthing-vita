//! HTTPS infrastructure connections: `HttpsServer` URL parsing, certificate pinning
//! (`PinnedOrWebPkiVerifier`, `SyncthingServerVerifier`, `SyncthingClientVerifier`), a real
//! pinned HTTPS exchange, the HTTP/1.1 response parser and the JSON helpers.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::tls_server::{
    spawn_tls_server, test_identity, try_read_http_request, write_http_response,
};
use rustls::client::danger::ServerCertVerifier;
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::ClientCertVerifier;
use rustls::RootCertStore;
use sha2::{Digest, Sha256};
use syncthing_core::discovery::http::{
    https_request, json_string_array, json_string_values, parse_http_response,
};
use syncthing_core::tls::https::HttpsServer;
use syncthing_core::tls::verifier::{
    PinnedOrWebPkiVerifier, SyncthingClientVerifier, SyncthingServerVerifier,
};
use syncthing_core::HttpsClient;
use syncthing_core::{DeviceId, DeviceIdentity};

const TIMEOUT: Duration = Duration::from_secs(5);

/// The `PinnedOrWebPkiVerifier` used by `open_https_stream`, with the public roots.
fn pinned_verifier(pin: Option<DeviceId>) -> PinnedOrWebPkiVerifier {
    let roots = Arc::new(RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    });
    let webpki = WebPkiServerVerifier::builder_with_provider(
        roots,
        Arc::new(syncthing_core::tls::crypto_provider()),
    )
    .build()
    .expect("webpki verifier");
    PinnedOrWebPkiVerifier::new(pin, webpki)
}

fn cert_of(identity: &DeviceIdentity) -> CertificateDer<'static> {
    CertificateDer::from(identity.cert_der.clone())
}

fn server_name() -> ServerName<'static> {
    ServerName::try_from("discovery.test").expect("server name")
}

// ---------------------------------------------------------------------------
// URL parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_defaults_to_port_443_and_root_path() {
    let server = HttpsServer::parse("https://example.org").expect("valid URL");
    assert_eq!(server.host, "example.org");
    assert_eq!(server.port, 443);
    assert_eq!(server.path, "/");
    assert_eq!(server.pinned_id, None);
}

#[test]
fn parse_keeps_explicit_port_and_path() {
    let server = HttpsServer::parse("https://example.org:8443/v2/status").expect("valid URL");
    assert_eq!(server.host, "example.org");
    assert_eq!(server.port, 8443);
    assert_eq!(server.path, "/v2/status");
    assert_eq!(server.pinned_id, None);
}

#[test]
fn parse_extracts_pin_and_drops_client_only_flags() {
    let id = DeviceId([0x42; 32]);
    for flag in ["nolookup", "noannounce", "insecure"] {
        let url = format!(
            "https://discovery.example/v2/?{flag}&id={}",
            id.to_syncthing_string()
        );
        let server = HttpsServer::parse(&url).expect("valid URL");
        assert_eq!(server.path, "/v2/", "flag {flag} must not reach the path");
        assert_eq!(server.pinned_id, Some(id), "flag {flag}");
    }
}

#[test]
fn parse_keeps_server_query_parameters() {
    let server =
        HttpsServer::parse("https://discovery.example/v2/?device=ABC&nolookup").expect("valid URL");
    assert_eq!(server.path, "/v2/?device=ABC");
    assert_eq!(server.pinned_id, None);
}

#[test]
fn parse_pin_without_path_uses_root_path() {
    let id = DeviceId([0x43; 32]);
    let url = format!("https://discovery.example?id={}", id.to_syncthing_string());
    let server = HttpsServer::parse(&url).expect("valid URL");
    assert_eq!(server.host, "discovery.example");
    assert_eq!(server.port, 443);
    assert_eq!(server.path, "/");
    assert_eq!(server.pinned_id, Some(id));
}

#[test]
fn parse_ipv6_literal_hosts() {
    let with_port = HttpsServer::parse("https://[::1]:8443/v2/").expect("valid URL");
    assert_eq!(with_port.host, "::1");
    assert_eq!(with_port.port, 8443);
    assert_eq!(with_port.path, "/v2/");

    let default_port = HttpsServer::parse("https://[2001:db8::5]/").expect("valid URL");
    assert_eq!(default_port.host, "2001:db8::5");
    assert_eq!(default_port.port, 443);
    assert_eq!(default_port.path, "/");
}

#[test]
fn parse_trims_surrounding_whitespace() {
    let server = HttpsServer::parse("  https://example.org:9/  ").expect("valid URL");
    assert_eq!(server.host, "example.org");
    assert_eq!(server.port, 9);
}

#[test]
fn parse_rejects_invalid_urls() {
    let id = DeviceId([0x44; 32]).to_syncthing_string();
    let invalid = [
        "http://example.org/".to_owned(),
        "example.org".to_owned(),
        "https://".to_owned(),
        "https://example.org:notaport/".to_owned(),
        "https://example.org:/".to_owned(),
        "https://example.org:99999/".to_owned(),
        "https://example.org/?id=not$a!device".to_owned(),
        // A short ID can never match the full certificate hash.
        "https://example.org/?id=MFZWI3D".to_owned(),
        "https://example.org/?id=".to_owned(),
        format!("https://[::1/?id={id}"),
        "https://[::1]x/".to_owned(),
        "https://[::1]:/".to_owned(),
    ];
    for url in &invalid {
        assert!(HttpsServer::parse(url).is_none(), "accepted {url:?}");
    }
}

#[test]
fn host_header_omits_default_port() {
    let default = HttpsServer::parse("https://example.org/").expect("valid URL");
    assert_eq!(default.host_header(), "example.org");

    let explicit_default = HttpsServer::parse("https://example.org:443/").expect("valid URL");
    assert_eq!(explicit_default.host_header(), "example.org");

    let custom = HttpsServer::parse("https://example.org:8443/").expect("valid URL");
    assert_eq!(custom.host_header(), "example.org:8443");
}

#[test]
fn host_header_brackets_ipv6_literals() {
    let with_port = HttpsServer::parse("https://[::1]:8443/").expect("valid URL");
    assert_eq!(with_port.host_header(), "[::1]:8443");

    let default_port = HttpsServer::parse("https://[::1]/").expect("valid URL");
    assert_eq!(default_port.host_header(), "[::1]");
}

#[test]
fn parse_rejects_empty_host() {
    assert!(HttpsServer::parse("https://:443/").is_none());
}

// ---------------------------------------------------------------------------
// Certificate verification
// ---------------------------------------------------------------------------

#[test]
fn pinned_verifier_accepts_the_pinned_self_signed_certificate() {
    let peer = test_identity(0x41);
    let verifier = pinned_verifier(Some(peer.device_id));
    verifier
        .verify_server_cert(&cert_of(&peer), &[], &server_name(), &[], UnixTime::now())
        .expect("pinned certificate accepted");
}

#[test]
fn pinned_verifier_rejects_another_self_signed_certificate() {
    let presented = test_identity(0x42);
    let pinned = test_identity(0x43);
    let verifier = pinned_verifier(Some(pinned.device_id));
    assert!(verifier
        .verify_server_cert(
            &cert_of(&presented),
            &[],
            &server_name(),
            &[],
            UnixTime::now()
        )
        .is_err());
}

#[test]
fn pinned_verifier_without_pin_rejects_self_signed_certificate() {
    let peer = test_identity(0x44);
    let verifier = pinned_verifier(None);
    assert!(verifier
        .verify_server_cert(&cert_of(&peer), &[], &server_name(), &[], UnixTime::now())
        .is_err());
}

#[test]
fn syncthing_server_verifier_pins_and_records_the_device_id() {
    let peer = test_identity(0x45);
    let other = test_identity(0x46);
    let provider = Arc::new(syncthing_core::tls::crypto_provider());

    let observed = Arc::new(Mutex::new(None));
    let wrong = SyncthingServerVerifier::new(
        Arc::clone(&provider),
        Some(other.device_id),
        Arc::clone(&observed),
    );
    assert!(wrong
        .verify_server_cert(&cert_of(&peer), &[], &server_name(), &[], UnixTime::now())
        .is_err());
    assert_eq!(
        *observed.lock().unwrap(),
        None,
        "a rejected peer is not recorded"
    );

    let right = SyncthingServerVerifier::new(provider, Some(peer.device_id), Arc::clone(&observed));
    right
        .verify_server_cert(&cert_of(&peer), &[], &server_name(), &[], UnixTime::now())
        .expect("pinned peer accepted");
    assert_eq!(*observed.lock().unwrap(), Some(peer.device_id));
}

#[test]
fn syncthing_client_verifier_accepts_any_self_signed_certificate_and_records_its_id() {
    let client = test_identity(0x47);
    let observed = Arc::new(Mutex::new(None));
    let verifier = SyncthingClientVerifier::new(
        Arc::new(syncthing_core::tls::crypto_provider()),
        Arc::clone(&observed),
    );
    verifier
        .verify_client_cert(&cert_of(&client), &[], UnixTime::now())
        .expect("self-signed client certificate accepted");

    // The DeviceId is the SHA-256 of the certificate.
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&Sha256::digest(&client.cert_der));
    assert_eq!(client.device_id, DeviceId(digest));
    assert_eq!(*observed.lock().unwrap(), Some(client.device_id));
}

// ---------------------------------------------------------------------------
// Real HTTPS exchanges against a local pinned server
// ---------------------------------------------------------------------------

#[test]
fn https_request_accepts_the_pinned_server() {
    let server_identity = test_identity(0x11);
    let server = spawn_tls_server(server_identity, b"http/1.1", false, |stream| {
        let request = try_read_http_request(stream).expect("request");
        write_http_response(
            stream,
            "200 OK",
            &[("Content-Type", "application/json")],
            "{\"addresses\":[]}",
        )
        .expect("response");
        request
    });
    let url = format!(
        "https://127.0.0.1:{}/v2/?nolookup&id={}",
        server.port,
        server.device_id.to_syncthing_string()
    );
    let target = HttpsServer::parse(&url).expect("valid URL");
    let response = https_request(
        &target,
        &HttpsClient::default(),
        "GET",
        "/v2/?device=X",
        None,
        None,
        TIMEOUT,
    )
    .expect("pinned server accepted");
    let (request, _) = server.join();

    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));
    assert_eq!(response.body, "{\"addresses\":[]}");
    assert_eq!(request.request_line(), "GET /v2/?device=X HTTP/1.1");
    assert!(request.head.contains("Host: 127.0.0.1:"));
}

#[test]
fn https_request_rejects_a_server_with_another_pin() {
    let server = spawn_tls_server(test_identity(0x11), b"http/1.1", false, |stream| {
        try_read_http_request(stream).is_ok()
    });
    let wrong_pin = test_identity(0x12).device_id;
    let url = format!(
        "https://127.0.0.1:{}/?id={}",
        server.port,
        wrong_pin.to_syncthing_string()
    );
    let target = HttpsServer::parse(&url).expect("valid URL");
    let result = https_request(
        &target,
        &HttpsClient::default(),
        "GET",
        "/",
        None,
        None,
        TIMEOUT,
    );
    assert!(
        result.is_err(),
        "a certificate that does not match the pin must fail"
    );
    let (reached, _) = server.join();
    assert!(!reached, "the request reached the server");
}

#[test]
fn https_request_rejects_an_unpinned_self_signed_server() {
    let server = spawn_tls_server(test_identity(0x13), b"http/1.1", false, |stream| {
        try_read_http_request(stream).is_ok()
    });
    let url = format!("https://127.0.0.1:{}/", server.port);
    let target = HttpsServer::parse(&url).expect("valid URL");
    assert!(https_request(
        &target,
        &HttpsClient::default(),
        "GET",
        "/",
        None,
        None,
        TIMEOUT
    )
    .is_err());
    let (reached, _) = server.join();
    assert!(!reached);
}

// ---------------------------------------------------------------------------
// HTTP/1.1 response parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_http_response_reads_status_headers_and_body() {
    let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nReannounce-After: 1800\r\n\r\n{\"a\":1}";
    let response = parse_http_response(raw).expect("valid response");
    assert_eq!(response.status, 200);
    assert_eq!(response.headers.len(), 2);
    assert_eq!(response.body, "{\"a\":1}");
}

#[test]
fn http_header_lookup_is_case_insensitive() {
    let raw = "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 60\r\n\r\n";
    let response = parse_http_response(raw).expect("valid response");
    assert_eq!(response.header("retry-after"), Some("60"));
    assert_eq!(response.header("RETRY-AFTER"), Some("60"));
    assert_eq!(response.header("Reannounce-After"), None);
}

#[test]
fn http_header_values_keep_their_colons() {
    let raw = "HTTP/1.1 301 Moved\r\nLocation: https://example.org:8443/v2/\r\n\r\n";
    let response = parse_http_response(raw).expect("valid response");
    assert_eq!(
        response.header("location"),
        Some("https://example.org:8443/v2/")
    );
}

#[test]
fn parse_http_response_without_body() {
    let response = parse_http_response("HTTP/1.1 204 No Content\r\nReannounce-After: 1800\r\n\r\n")
        .expect("valid response");
    assert_eq!(response.status, 204);
    assert_eq!(response.body, "");
}

#[test]
fn parse_http_response_decodes_chunked_bodies() {
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n6;ext=1\r\n world\r\n0\r\n\r\n";
    let response = parse_http_response(raw).expect("valid response");
    assert_eq!(response.body, "Hello world");
}

#[test]
fn parse_http_response_truncated_chunk_does_not_panic() {
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA\r\nshort";
    let response = parse_http_response(raw).expect("valid response");
    assert_eq!(response.body, "short");
}

#[test]
fn parse_http_response_chunk_size_inside_a_character_does_not_panic() {
    // "ä" is two bytes: a chunk size of 1 ends inside the character.
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\n\u{e4}\r\n";
    let response = parse_http_response(raw).expect("valid response");
    assert!(response.body.starts_with('\u{e4}'));
}

#[test]
fn parse_http_response_invalid_chunk_size_ends_the_body() {
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nabc";
    let response = parse_http_response(raw).expect("valid response");
    assert_eq!(response.body, "");
}

#[test]
fn parse_http_response_rejects_malformed_status_lines() {
    assert!(parse_http_response("garbage").is_none());
    assert!(parse_http_response("HTTP/1.1 abc OK\r\n\r\n").is_none());
    assert!(parse_http_response("").is_none());
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

#[test]
fn json_string_array_reads_a_flat_list_with_spacing() {
    let json = r#"{ "addresses" : [ "tcp://a:1" , "relay://b:2/?id=X" ] }"#;
    assert_eq!(
        json_string_array(json, "addresses"),
        vec!["tcp://a:1".to_owned(), "relay://b:2/?id=X".to_owned()]
    );
}

#[test]
fn json_string_array_missing_or_empty_is_empty() {
    assert!(json_string_array(r#"{"other":["x"]}"#, "addresses").is_empty());
    assert!(json_string_array(r#"{"addresses":[]}"#, "addresses").is_empty());
    assert!(json_string_array(r#"{"addresses":null}"#, "addresses").is_empty());
}

#[test]
fn json_string_array_skips_empty_strings_and_unescapes_quotes() {
    let json = r#"{"addresses":["", "a\"b"]}"#;
    assert_eq!(
        json_string_array(json, "addresses"),
        vec!["a\"b".to_owned()]
    );
}

#[test]
fn json_string_array_keeps_brackets_inside_strings() {
    let json = r#"{"addresses":["tcp://[::1]:22000"]}"#;
    assert_eq!(
        json_string_array(json, "addresses"),
        vec!["tcp://[::1]:22000".to_owned()]
    );
}

#[test]
fn json_string_values_collects_every_match_and_skips_non_strings() {
    let json = r#"{"relays":[{"url":"relay://1","location":{}},{"url":"relay://2"}],"n":{"url":5},"m":{"url":"relay://3"}}"#;
    assert_eq!(
        json_string_values(json, "url"),
        vec![
            "relay://1".to_owned(),
            "relay://2".to_owned(),
            "relay://3".to_owned()
        ]
    );
}

#[test]
fn json_string_values_unescapes_slashes() {
    let json = r#"{"url":"relay:\/\/198.51.100.5:22067\/?id=X"}"#;
    assert_eq!(
        json_string_values(json, "url"),
        vec!["relay://198.51.100.5:22067/?id=X".to_owned()]
    );
}

#[test]
fn json_string_values_missing_key_is_empty() {
    assert!(json_string_values(r#"{"relays":[]}"#, "url").is_empty());
}

// ---------------------------------------------------------------------------
// Address family filtering for connect_any
// ---------------------------------------------------------------------------

#[test]
fn select_connect_addrs_filters_ipv6_when_ipv4_only_and_preserves_order() {
    use std::net::SocketAddr;
    use syncthing_core::tls::https::select_connect_addrs;

    let v6_a: SocketAddr = "[2001:db8::1]:443".parse().unwrap();
    let v4_a: SocketAddr = "192.0.2.1:443".parse().unwrap();
    let v6_b: SocketAddr = "[::1]:443".parse().unwrap();
    let v4_b: SocketAddr = "198.51.100.2:443".parse().unwrap();
    let addrs = vec![v6_a, v4_a, v6_b, v4_b];

    assert_eq!(select_connect_addrs(addrs.clone(), true), vec![v4_a, v4_b]);
    assert_eq!(
        select_connect_addrs(addrs, false),
        vec![v6_a, v4_a, v6_b, v4_b]
    );
    assert!(select_connect_addrs(vec![v6_a, v6_b], true).is_empty());
}
