//! Relay pool download (`relay::pool::fetch_relay_pool`) from a local pinned HTTPS server:
//! the `relay://` URLs of the answer are returned, nothing else; other statuses are errors.

mod common;

use std::time::Duration;

use common::tls_server::{
    spawn_tls_server, test_identity, try_read_http_request, write_http_response,
};
use syncthing_core::relay::pool::fetch_relay_pool;
use syncthing_core::{HttpsClient, HttpsServer};

const TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_URL: &str = "relay://198.51.100.5:22067/?id=ABCDEFG";

fn pool_url(port: u16, id: syncthing_core::DeviceId) -> HttpsServer {
    let url = format!(
        "https://127.0.0.1:{port}/endpoint?id={}",
        id.to_syncthing_string()
    );
    HttpsServer::parse(&url).expect("valid URL")
}

#[test]
fn the_relay_urls_of_the_pool_are_returned() {
    let body = format!(
        "{{\"relays\":[{{\"url\":\"{RELAY_URL}\",\"latency\":12}},{{\"url\":\"https://not-a-relay.example/\"}}]}}"
    );
    let server = spawn_tls_server(test_identity(0xA1), b"http/1.1", false, move |stream| {
        let request = try_read_http_request(stream).expect("request");
        write_http_response(
            stream,
            "200 OK",
            &[("Content-Type", "application/json")],
            &body,
        )
        .expect("response");
        request
    });
    let pool = pool_url(server.port, server.device_id);
    let relays = fetch_relay_pool(&pool, &HttpsClient::default(), TIMEOUT).expect("pool");
    let (request, _) = server.join();

    assert_eq!(relays, vec![RELAY_URL.to_owned()]);
    assert!(
        request.request_line().starts_with("GET /endpoint HTTP/1.1"),
        "{}",
        request.request_line()
    );
}

#[test]
fn an_empty_pool_is_an_empty_list() {
    let server = spawn_tls_server(test_identity(0xA2), b"http/1.1", false, |stream| {
        try_read_http_request(stream).expect("request");
        write_http_response(stream, "200 OK", &[], "{\"relays\":[]}").expect("response");
    });
    let pool = pool_url(server.port, server.device_id);
    let relays = fetch_relay_pool(&pool, &HttpsClient::default(), TIMEOUT).expect("pool");
    server.join();
    assert!(relays.is_empty(), "{relays:?}");
}

#[test]
fn a_pool_answer_other_than_200_is_an_error() {
    let server = spawn_tls_server(test_identity(0xA3), b"http/1.1", false, |stream| {
        try_read_http_request(stream).expect("request");
        write_http_response(stream, "503 Service Unavailable", &[], "").expect("response");
    });
    let pool = pool_url(server.port, server.device_id);
    let err = fetch_relay_pool(&pool, &HttpsClient::default(), TIMEOUT).expect_err("503");
    server.join();
    assert!(err.to_string().contains("503"), "{err}");
}
