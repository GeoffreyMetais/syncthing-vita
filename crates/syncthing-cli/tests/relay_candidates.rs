//! Relays to try, in order: the configured ones, or the public pool (fetched over HTTPS,
//! pinned to its certificate) in a rotated order.

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use syncthing_cli::config::Config;
use syncthing_cli::daemon::context::{DaemonContext, DaemonOptions};
use syncthing_cli::daemon::relay::candidate_relays;
use syncthing_core::{DeviceIdentity, HttpsClient};

use common::{free_port, temp_home};

const RELAY_A: &str = "relay://198.51.100.1:22067/?id=AAAA";
const RELAY_B: &str = "relay://198.51.100.2:22067/?id=BBBB";
const RELAY_C: &str = "relay://198.51.100.3:22067/?id=CCCC";

fn context(static_relays: Vec<String>, pool_url: String) -> DaemonContext {
    let options = DaemonOptions {
        listen: None,
        local_discovery: false,
        global_discovery: false,
        relays: true,
        announce_servers: Vec::new(),
        lookup_servers: Vec::new(),
        relay_pool_url: pool_url,
        static_relays,
        https: HttpsClient::default(),
    };
    DaemonContext::new(
        &temp_home("relay-candidates"),
        &Config::default(),
        DeviceIdentity::from_seed(&[3; 32]).expect("identity"),
        options,
    )
}

/// Serves `body` as the relay pool to one HTTPS connection, from a certificate whose hash is
/// the returned pin. Returns the pool URL (pinned) once listening.
fn serve_pool(body: &'static str) -> String {
    let identity = DeviceIdentity::from_seed(&[60; 32]).expect("identity");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    let provider = Arc::new(rustls_rustcrypto::provider());
    let config = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(identity.cert_der.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone())),
        )
        .expect("server certificate");
    let config = Arc::new(config);
    thread::spawn(move || {
        let (tcp, _) = listener.accept().expect("accept");
        let conn = ServerConnection::new(config).expect("server connection");
        let mut stream = StreamOwned::new(conn, tcp);
        // Read the request head, then answer with the pool.
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0u8; 1];
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                return;
            }
            head.push(byte[0]);
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        stream.conn.send_close_notify();
        let _ = stream.flush();
    });
    format!(
        "https://127.0.0.1:{port}/?id={}",
        identity.device_id.to_syncthing_string()
    )
}

#[test]
fn configured_relays_are_used_as_they_are() {
    let ctx = context(vec![RELAY_B.into(), RELAY_A.into()], String::new());
    assert_eq!(
        candidate_relays(&ctx),
        vec![RELAY_B.to_owned(), RELAY_A.to_owned()]
    );
}

#[test]
fn an_invalid_pool_url_yields_no_relay() {
    let ctx = context(Vec::new(), "http://relays.example/".into());
    assert!(candidate_relays(&ctx).is_empty());
}

#[test]
fn an_unreachable_pool_yields_no_relay() {
    let url = format!("https://127.0.0.1:{}/", free_port());
    let ctx = context(Vec::new(), url);
    assert!(candidate_relays(&ctx).is_empty());
}

#[test]
fn an_empty_pool_yields_no_relay() {
    let url = serve_pool("[]");
    let ctx = context(Vec::new(), url);
    assert!(candidate_relays(&ctx).is_empty());
}

#[test]
fn the_pool_keeps_only_relay_uris_in_a_rotation_of_its_order() {
    let url = serve_pool(
        r#"[{"url":"relay://198.51.100.1:22067/?id=AAAA"},{"url":"https://not-a-relay.example/"},{"url":"relay://198.51.100.2:22067/?id=BBBB"},{"url":"relay://198.51.100.3:22067/?id=CCCC"}]"#,
    );
    let ctx = context(Vec::new(), url);
    let found = candidate_relays(&ctx);
    let original = [RELAY_A, RELAY_B, RELAY_C];
    assert_eq!(found.len(), 3, "{found:?}");
    let is_rotation = (0..3).any(|k| (0..3).all(|i| found[i] == original[(i + k) % 3]));
    assert!(is_rotation, "{found:?} is not a rotation of {original:?}");
}
