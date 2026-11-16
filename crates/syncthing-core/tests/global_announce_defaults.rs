//! Global Discovery announcement timings when the server sends no `Reannounce-After` or
//! `Retry-After` header: the documented defaults, checked against literal durations.

mod common;

use std::time::Duration;

use common::tls_server::{
    spawn_tls_server, test_identity, try_read_http_request, write_http_response,
};
use syncthing_core::discovery::global_announce::{announce_on, AnnounceOutcome};
use syncthing_core::{HttpsClient, HttpsServer};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Announces to a local pinned server answering `status` with no timing header.
fn announce_against(seed: u8, status: &'static str) -> AnnounceOutcome {
    let server = spawn_tls_server(test_identity(seed), b"http/1.1", true, move |stream| {
        try_read_http_request(stream).expect("announcement");
        write_http_response(stream, status, &[], "").expect("response");
    });
    let url = format!(
        "https://127.0.0.1:{}/v2/?nolookup&id={}",
        server.port,
        server.device_id.to_syncthing_string()
    );
    let target = HttpsServer::parse(&url).expect("valid URL");
    let outcome = announce_on(
        &target,
        &HttpsClient::default(),
        &test_identity(0x95),
        &[],
        TIMEOUT,
    )
    .expect("announce");
    server.join();
    outcome
}

#[test]
fn an_accepted_announcement_without_header_is_repeated_after_30_minutes() {
    assert_eq!(
        announce_against(0x96, "204 No Content"),
        AnnounceOutcome::Accepted {
            reannounce_after: Duration::from_secs(30 * 60),
        }
    );
}

#[test]
fn a_refused_announcement_without_header_is_retried_after_5_minutes() {
    assert_eq!(
        announce_against(0x97, "429 Too Many Requests"),
        AnnounceOutcome::Refused {
            status: 429,
            retry_after: Duration::from_secs(5 * 60),
        }
    );
}
