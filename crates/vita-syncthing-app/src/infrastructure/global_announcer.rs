//! Global Discovery v2 announcer for WAN reachability.
//!
//! Periodically announces our reachable addresses (`tcp://0.0.0.0:<port>` and optional
//! `relay://...` URI from [`SharedRelayUri`]) to Syncthing's public Global Discovery v2 servers.
//! On PS Vita (`SceNet` is IPv4-only), only the IPv4 announce server is queried.

use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use syncthing_core::discovery::global_announce::DEFAULT_ANNOUNCE_RETRY;
use syncthing_core::{
    announce_on, AnnounceOutcome, DeviceIdentity, HttpsClient, HttpsServer, SyncControl,
    DEFAULT_ANNOUNCE_SERVERS,
};

use super::peer_endpoints::sleep_interruptible;
use super::relay_listener::{get_shared_relay_uri, SharedRelayUri};
use super::sync_worker::WorkerMsg;
use crate::domain::defaults::{GLOBAL_ANNOUNCE_POLL, GLOBAL_DISCOVERY_TIMEOUT};

/// Shared context passed to [`run_global_announcer`] by `spawn_sync_worker` (or tests).
#[derive(Clone)]
pub struct GlobalAnnouncerContext {
    pub identity: DeviceIdentity,
    pub listen_port: Option<u16>,
    pub shared_relay_uri: SharedRelayUri,
    pub control: Arc<SyncControl>,
    pub tx: Sender<WorkerMsg>,
}

/// Builds the list of addresses to announce on Global Discovery v2:
/// `tcp://0.0.0.0:<port>` (the server substitutes our public IP) and optional `relay://...`.
pub fn build_announced_addresses(listen_port: Option<u16>, relay_uri: Option<&str>) -> Vec<String> {
    let mut addresses = Vec::new();
    if let Some(port) = listen_port.filter(|&p| p != 0) {
        addresses.push(format!("tcp://0.0.0.0:{port}"));
    }
    if let Some(uri) = relay_uri.map(str::trim).filter(|s| !s.is_empty()) {
        addresses.push(uri.to_owned());
    }
    addresses
}

/// Returns the Global Discovery v2 announce server URLs for the given IPv4-only constraint.
pub fn announce_servers(ipv4_only: bool) -> &'static [&'static str] {
    if ipv4_only {
        &DEFAULT_ANNOUNCE_SERVERS[..1]
    } else {
        &DEFAULT_ANNOUNCE_SERVERS
    }
}

/// Default Global Discovery v2 announce servers: IPv4-only on PS Vita (`SceNet` has no IPv6),
/// both IPv4 and IPv6 on host.
pub fn default_announce_servers() -> &'static [&'static str] {
    announce_servers(cfg!(target_os = "vita"))
}

/// Runs the Global Discovery v2 announcer against [`default_announce_servers`] until
/// `ctx.control.is_stop_requested()`.
pub fn run_global_announcer(ctx: GlobalAnnouncerContext) {
    let servers: Vec<HttpsServer> = default_announce_servers()
        .iter()
        .filter_map(|url| HttpsServer::parse(url))
        .collect();
    if servers.is_empty() {
        return;
    }
    let client = HttpsClient::default();
    run_global_announcer_on(&servers, &client, &ctx);
}

/// Runs the Global Discovery v2 announcer loop against `servers` using `client` until
/// `ctx.control.is_stop_requested()`.
pub fn run_global_announcer_on(
    servers: &[HttpsServer],
    client: &HttpsClient,
    ctx: &GlobalAnnouncerContext,
) {
    let mut next_due: Vec<Option<Instant>> = vec![None; servers.len()];
    let mut last_announced_log: Vec<Option<Vec<String>>> = vec![None; servers.len()];
    let mut last_error_log: Vec<Option<String>> = vec![None; servers.len()];
    let mut announced: Vec<String> = Vec::new();

    while !ctx.control.is_stop_requested() {
        let relay_uri = get_shared_relay_uri(&ctx.shared_relay_uri);
        let addresses = build_announced_addresses(ctx.listen_port, relay_uri.as_deref());
        let changed = addresses != announced;

        for (idx, server) in servers.iter().enumerate() {
            if ctx.control.is_stop_requested() {
                break;
            }
            if addresses.is_empty() {
                continue;
            }
            let due = next_due[idx].is_none_or(|t| Instant::now() >= t);
            if !changed && !due {
                continue;
            }

            match announce_on(
                server,
                client,
                &ctx.identity,
                &addresses,
                GLOBAL_DISCOVERY_TIMEOUT,
            ) {
                Ok(AnnounceOutcome::Accepted { reannounce_after }) => {
                    last_error_log[idx] = None;
                    if last_announced_log[idx].as_deref() != Some(addresses.as_slice()) {
                        last_announced_log[idx] = Some(addresses.clone());
                        let _ = ctx.tx.send(WorkerMsg::Log(format!(
                            "Global Discovery: announced {} on {}",
                            addresses.join(", "),
                            server.host
                        )));
                    }
                    next_due[idx] = Some(Instant::now() + reannounce_after);
                }
                Ok(AnnounceOutcome::Refused {
                    status,
                    retry_after,
                }) => {
                    let err_key = format!("HTTP {status}");
                    if last_error_log[idx].as_deref() != Some(err_key.as_str()) {
                        last_error_log[idx] = Some(err_key);
                        let _ = ctx.tx.send(WorkerMsg::Log(format!(
                            "Global Discovery: {} refused announcement (HTTP {status})",
                            server.host
                        )));
                    }
                    next_due[idx] = Some(Instant::now() + retry_after);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if last_error_log[idx].as_deref() != Some(err_str.as_str()) {
                        last_error_log[idx] = Some(err_str.clone());
                        let _ = ctx.tx.send(WorkerMsg::Log(format!(
                            "Global Discovery: cannot announce on {}: {err_str}",
                            server.host
                        )));
                    }
                    next_due[idx] = Some(Instant::now() + DEFAULT_ANNOUNCE_RETRY);
                }
            }
        }

        announced = addresses;

        if !sleep_interruptible(&ctx.control, GLOBAL_ANNOUNCE_POLL) {
            break;
        }
    }
}
