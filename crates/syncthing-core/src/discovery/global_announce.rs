//! Syncthing Global Discovery v2 announcements: tells the discovery servers at which
//! addresses (`tcp://`, `relay://`) this device can be reached from the Internet.
//!
//! The server identifies us by the certificate we present (mutual TLS); the body only lists
//! addresses. An unspecified IP (`tcp://0.0.0.0:22000`) is replaced by the server with the
//! address the announcement came from.

use std::io;
use std::time::Duration;

use super::http::https_request;
use crate::cert::DeviceIdentity;
use crate::tls::https::HttpsServer;

/// Public Syncthing Global Discovery v2 announce servers (IPv4, IPv6), certificate pinned.
pub const DEFAULT_ANNOUNCE_SERVERS: [&str; 2] = [
    concat!(
        "https://discovery-announce-v4.syncthing.net/v2/?nolookup&id=",
        "LYXKCHX-VI3NYZR-ALCJBHF-WMZYSPK-QG6QJA3-MPFYMSO-U56GTUK-NA2MIAW"
    ),
    concat!(
        "https://discovery-announce-v6.syncthing.net/v2/?nolookup&id=",
        "LYXKCHX-VI3NYZR-ALCJBHF-WMZYSPK-QG6QJA3-MPFYMSO-U56GTUK-NA2MIAW"
    ),
];

/// Re-announcement interval when the server does not say (`Reannounce-After`).
pub const DEFAULT_REANNOUNCE_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// Retry delay after a failed announcement when the server does not say (`Retry-After`).
pub const DEFAULT_ANNOUNCE_RETRY: Duration = Duration::from_secs(5 * 60);

/// Outcome of an announcement: when to announce again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnounceOutcome {
    /// Accepted; announce again after this delay.
    Accepted { reannounce_after: Duration },
    /// Refused with this HTTP status (e.g. `429`); retry after this delay.
    Refused { status: u16, retry_after: Duration },
}

/// JSON body of an announcement.
pub fn encode_announcement(addresses: &[String]) -> String {
    let items: Vec<String> = addresses
        .iter()
        .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("{{\"addresses\":[{}]}}", items.join(","))
}

fn seconds_header(value: Option<&str>) -> Option<Duration> {
    value?.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// Announces `addresses` for `identity` on `server`. Network and TLS failures are errors; an
/// HTTP refusal is an [`AnnounceOutcome::Refused`].
pub fn announce_on(
    server: &HttpsServer,
    identity: &DeviceIdentity,
    addresses: &[String],
    timeout: Duration,
) -> io::Result<AnnounceOutcome> {
    let body = encode_announcement(addresses);
    let response = https_request(
        server,
        "POST",
        &server.path,
        Some(&body),
        Some(identity),
        timeout,
    )?;
    Ok(match response.status {
        200..=299 => AnnounceOutcome::Accepted {
            reannounce_after: seconds_header(response.header("Reannounce-After"))
                .unwrap_or(DEFAULT_REANNOUNCE_INTERVAL),
        },
        status => AnnounceOutcome::Refused {
            status,
            retry_after: seconds_header(response.header("Retry-After"))
                .or_else(|| seconds_header(response.header("Reannounce-After")))
                .unwrap_or(DEFAULT_ANNOUNCE_RETRY),
        },
    })
}
