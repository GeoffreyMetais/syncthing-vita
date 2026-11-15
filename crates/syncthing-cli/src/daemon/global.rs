//! Global Discovery announcements: our direct address (when listening) and the relay we are
//! reachable through, so that devices outside the LAN can connect to us.

use std::sync::Arc;
use std::time::{Duration, Instant};

use syncthing_core::{announce_on, AnnounceOutcome, HttpsServer};

use super::context::DaemonContext;
use super::log::log;
use super::timing::{HTTPS_TIMEOUT, RELAY_POLL};

/// Addresses to announce: `tcp://0.0.0.0:<port>` (the server substitutes the address the
/// announcement came from) and our relay URIs.
pub fn announced_addresses(ctx: &DaemonContext) -> Vec<String> {
    let mut addresses = Vec::new();
    if let Some(port) = *ctx.listen_port.lock().unwrap_or_else(|p| p.into_inner()) {
        addresses.push(format!("tcp://0.0.0.0:{port}"));
    }
    addresses.extend(ctx.table().relay_uris());
    addresses
}

/// Announces to every configured server, again when the server asks or our addresses change.
pub fn run_global_announcer(ctx: Arc<DaemonContext>) {
    let servers: Vec<HttpsServer> = ctx
        .options
        .announce_servers
        .iter()
        .filter_map(|url| {
            let parsed = HttpsServer::parse(url);
            if parsed.is_none() {
                log(&format!("Ignoring invalid announce server URL {url}"));
            }
            parsed
        })
        .collect();
    let mut next: Vec<Option<Instant>> = vec![None; servers.len()];
    let mut announced: Vec<String> = Vec::new();
    while !ctx.is_stopping() {
        let addresses = announced_addresses(&ctx);
        let changed = addresses != announced;
        for (server, due) in servers.iter().zip(next.iter_mut()) {
            if addresses.is_empty() || !(changed || due.is_none_or(|t| Instant::now() >= t)) {
                continue;
            }
            let delay = announce(&ctx, server, &addresses);
            *due = Some(Instant::now() + delay);
        }
        announced = addresses;
        ctx.sleep(RELAY_POLL);
    }
}

/// One announcement; returns the delay before the next one.
fn announce(ctx: &DaemonContext, server: &HttpsServer, addresses: &[String]) -> Duration {
    match announce_on(
        server,
        &ctx.options.https,
        &ctx.identity,
        addresses,
        HTTPS_TIMEOUT,
    ) {
        Ok(AnnounceOutcome::Accepted { reannounce_after }) => {
            if ctx.first_report(format!("announced:{}:{addresses:?}", server.host)) {
                log(&format!(
                    "Global Discovery: announced {} on {}",
                    addresses.join(", "),
                    server.host
                ));
            }
            reannounce_after
        }
        Ok(AnnounceOutcome::Refused {
            status,
            retry_after,
        }) => {
            log(&format!(
                "Global Discovery: {} refused the announcement (HTTP {status})",
                server.host
            ));
            retry_after
        }
        Err(e) => {
            if ctx.first_report(format!("announce-error:{}:{e}", server.host)) {
                log(&format!(
                    "Global Discovery: cannot announce on {}: {e}",
                    server.host
                ));
            }
            syncthing_core::discovery::global_announce::DEFAULT_ANNOUNCE_RETRY
        }
    }
}
