//! The public relay pool (`https://relays.syncthing.net/endpoint`): the list of relays a
//! device may join to be reachable from the Internet.

use std::io;
use std::time::Duration;

use crate::discovery::http::{https_request, json_string_values};
use crate::tls::https::HttpsServer;

/// Syncthing's public relay pool.
pub const DEFAULT_RELAY_POOL: &str = "https://relays.syncthing.net/endpoint";

/// The `relay://` URLs listed in a relay pool answer (`{"relays":[{"url":"relay://..."}]}`).
pub fn parse_relay_pool_json(json: &str) -> Vec<String> {
    json_string_values(json, "url")
        .into_iter()
        .filter(|u| u.starts_with("relay://"))
        .collect()
}

/// Downloads the relay list of `pool`.
pub fn fetch_relay_pool(pool: &HttpsServer, timeout: Duration) -> io::Result<Vec<String>> {
    let response = https_request(pool, "GET", &pool.path, None, None, timeout)?;
    if response.status != 200 {
        return Err(io::Error::other(format!(
            "Relay pool HTTP {}",
            response.status
        )));
    }
    Ok(parse_relay_pool_json(&response.body))
}
