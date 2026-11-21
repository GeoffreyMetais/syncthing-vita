//! Peer endpoint resolution: configured addresses (IP or DNS hostname), LAN UDP discovery,
//! and Syncthing Global Discovery v2 (`tcp://` + `relay://`).

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use syncthing_core::{
    connect_and_sync, connect_and_sync_stream, connect_via_relay, is_local_ip, lookup_global_peer,
    BepHello, DeviceId, DeviceIdentity, FsSyncStorage, PeerEndpoint, SessionParams, SyncControl,
    SyncEvent, SyncSessionStatus,
};

use super::session_guard::SessionClaims;
use super::sync_worker::WorkerMsg;
use crate::domain::defaults::{
    CONNECT_TIMEOUT, GLOBAL_DISCOVERY_TIMEOUT, NO_ADDRESS_POLL, RETRY_AFTER_ERROR,
    RETRY_AFTER_SESSION, RETRY_AWAITING_PAIRING,
};
use crate::domain::{KnownDevice, UiText};

/// Granularity at which a sleeping worker notices a stop request.
const STOP_CHECK_STEP: Duration = Duration::from_millis(200);

pub type SharedDiscoveredAddrs = Arc<Mutex<Vec<(DeviceId, Vec<SocketAddr>)>>>;
pub type SharedGlobalEndpoints = Arc<Mutex<Vec<(DeviceId, Vec<PeerEndpoint>)>>>;

/// A device is looked up on Global Discovery only when that can help and is possible: it has a
/// full `DeviceId` and no session is established with it. Looking up a connected peer would
/// only tell the discovery server which devices this console talks to.
pub fn needs_global_lookup(kd: &KnownDevice, connected: &[DeviceId]) -> bool {
    !kd.device_id.is_short_placeholder()
        && !connected.iter().any(|id| id.matches_peer(&kd.device_id))
}

/// Resolves a configured peer address (`ip:port` or `hostname:port`), returning empty for
/// `"dynamic"` or unparseable/unresolvable strings.
pub fn resolve_configured_addr(addr: &str) -> Vec<SocketAddr> {
    let trimmed = addr.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("dynamic") {
        return Vec::new();
    }
    if let Ok(sock) = trimmed.parse::<SocketAddr>() {
        return (!sock.ip().is_unspecified())
            .then_some(sock)
            .into_iter()
            .collect();
    }
    trimmed
        .to_socket_addrs()
        .map(|iter| iter.filter(|s| !s.ip().is_unspecified()).collect())
        .unwrap_or_default()
}

fn push_unique(out: &mut Vec<PeerEndpoint>, ep: PeerEndpoint) {
    if !out.contains(&ep) {
        out.push(ep);
    }
}

/// Collects candidate endpoints for `kd`, ordered with local/LAN direct TCP endpoints first,
/// followed by non-local/WAN direct endpoints, and relay endpoints last.
pub fn collect_peer_targets(
    kd: &KnownDevice,
    discovered: &SharedDiscoveredAddrs,
    global: &SharedGlobalEndpoints,
) -> (DeviceId, Vec<PeerEndpoint>) {
    let mut resolved_id = kd.device_id;
    let mut local_direct = Vec::new();
    let mut wan_direct = Vec::new();
    let mut relays = Vec::new();

    for sock in resolve_configured_addr(&kd.address) {
        let target = if is_local_ip(sock.ip()) {
            &mut local_direct
        } else {
            &mut wan_direct
        };
        push_unique(target, PeerEndpoint::Direct(sock));
    }

    if let Ok(guard) = discovered.lock() {
        for (disc_id, disc_socks) in guard.iter() {
            if kd.device_id.matches_peer(disc_id) {
                resolved_id = *disc_id;
                for &s in disc_socks {
                    push_unique(&mut local_direct, PeerEndpoint::Direct(s));
                }
            }
        }
    }

    if let Ok(guard) = global.lock() {
        for (glob_id, eps) in guard.iter() {
            if kd.device_id.matches_peer(glob_id) {
                resolved_id = *glob_id;
                for ep in eps {
                    match ep {
                        PeerEndpoint::Direct(sock) => {
                            let target = if is_local_ip(sock.ip()) {
                                &mut local_direct
                            } else {
                                &mut wan_direct
                            };
                            push_unique(target, ep.clone());
                        }
                        PeerEndpoint::Relay { .. } => push_unique(&mut relays, ep.clone()),
                    }
                }
            }
        }
    }

    for ep in wan_direct {
        push_unique(&mut local_direct, ep);
    }
    for ep in relays {
        push_unique(&mut local_direct, ep);
    }
    (resolved_id, local_direct)
}

/// Queries Global Discovery v2 for every configured peer that needs it (see
/// [`needs_global_lookup`]) and updates the shared endpoint cache.
pub fn refresh_global_endpoints(
    approved_devices: &[KnownDevice],
    global: &SharedGlobalEndpoints,
    claims: &Arc<SessionClaims>,
    control: &SyncControl,
    tx: &Sender<WorkerMsg>,
    last_errors: &mut Vec<(DeviceId, String)>,
) {
    for kd in approved_devices {
        if control.is_stop_requested() {
            return;
        }
        let connected_now = claims.claimed_ids();
        if !needs_global_lookup(kd, &connected_now) {
            continue;
        }
        match lookup_global_peer(kd.device_id, GLOBAL_DISCOVERY_TIMEOUT) {
            Ok(eps) => {
                if let Some(slot) = last_errors.iter_mut().find(|(id, _)| *id == kd.device_id) {
                    slot.1.clear();
                }
                if let Ok(mut guard) = global.lock() {
                    if let Some(slot) = guard.iter_mut().find(|(id, _)| *id == kd.device_id) {
                        slot.1 = eps;
                    } else {
                        guard.push((kd.device_id, eps));
                    }
                }
            }
            Err(e) => {
                let msg = format!("Global discovery lookup for {} failed: {}", kd.name, e);
                let prev = last_errors.iter_mut().find(|(id, _)| *id == kd.device_id);
                match prev {
                    Some(slot) if slot.1 == msg => {}
                    Some(slot) => {
                        slot.1 = msg.clone();
                        let _ = tx.send(WorkerMsg::Log(msg));
                    }
                    None => {
                        last_errors.push((kd.device_id, msg.clone()));
                        let _ = tx.send(WorkerMsg::Log(msg));
                    }
                }
            }
        }
    }
}

/// Sleeps up to `duration`, checking every `STOP_CHECK_STEP` whether to give up early: on a
/// stop request (returns `false`) or when `wake` returns `true` (returns `true`).
/// Returns `true` if the caller should carry on.
pub fn sleep_unless_stopped(
    control: &SyncControl,
    duration: Duration,
    wake: impl Fn() -> bool,
) -> bool {
    let mut elapsed = Duration::ZERO;
    while elapsed < duration {
        if control.is_stop_requested() {
            return false;
        }
        if wake() {
            return true;
        }
        let step = (duration - elapsed).min(STOP_CHECK_STEP);
        std::thread::sleep(step);
        elapsed += step;
    }
    !control.is_stop_requested()
}

pub fn sleep_interruptible(control: &SyncControl, duration: Duration) -> bool {
    sleep_unless_stopped(control, duration, || false)
}

pub fn format_endpoint_label(ep: &PeerEndpoint) -> String {
    match ep {
        PeerEndpoint::Direct(addr) => addr.to_string(),
        PeerEndpoint::Relay { addr, .. } => format!("relay://{addr}"),
    }
}

/// What the peer session threads of one sync worker share.
#[derive(Clone)]
pub struct PeerSessionShared {
    pub identity: DeviceIdentity,
    pub hello: BepHello,
    pub storage: FsSyncStorage,
    pub discovered: SharedDiscoveredAddrs,
    pub global: SharedGlobalEndpoints,
    pub claims: Arc<SessionClaims>,
    pub initial_lan_done: Arc<AtomicBool>,
    pub control: Arc<SyncControl>,
    pub tx: Sender<WorkerMsg>,
}

/// Keeps a session with `kd` running until a stop is requested: tries its known endpoints in
/// turn (local/LAN direct ones first, then WAN direct, then relays) and retries after each
/// failure or session end.
pub fn run_peer_session_loop(kd: KnownDevice, shared: PeerSessionShared) {
    let PeerSessionShared {
        identity,
        hello,
        storage,
        discovered,
        global,
        claims,
        initial_lan_done,
        control,
        tx,
    } = shared;
    let mut logged_awaiting = false;
    let mut logged_no_addr = false;
    let mut report_connecting = true;
    let mut report_unreachable = true;
    let mut last_error = String::new();
    while !control.is_stop_requested() {
        if claims.is_claimed(&kd.device_id) {
            if !sleep_interruptible(&control, RETRY_AFTER_SESSION) {
                break;
            }
            continue;
        }

        let (target_id, targets) = collect_peer_targets(&kd, &discovered, &global);
        if targets.is_empty() {
            if !logged_no_addr {
                logged_no_addr = true;
                let _ = tx.send(WorkerMsg::Log(format!(
                    "No address for {} (configured: '{}'). Waiting for discovery...",
                    kd.name, kd.address
                )));
            }
            if !sleep_interruptible(&control, NO_ADDRESS_POLL) {
                break;
            }
            continue;
        }

        let has_local_target = targets
            .iter()
            .any(|ep| matches!(ep, PeerEndpoint::Direct(sock) if is_local_ip(sock.ip())));
        if !has_local_target && !initial_lan_done.load(Ordering::Acquire) {
            if !sleep_unless_stopped(&control, NO_ADDRESS_POLL, || {
                initial_lan_done.load(Ordering::Acquire)
            }) {
                break;
            }
            continue;
        }

        let mut connected_once = false;
        let mut awaiting_pairing = false;

        for ep in targets {
            if control.is_stop_requested() {
                return;
            }
            let Some(_claim) = claims.try_claim(target_id) else {
                connected_once = true;
                break;
            };
            let ep_label = format_endpoint_label(&ep);
            if report_connecting {
                report_connecting = false;
                let _ = tx.send(WorkerMsg::Progress {
                    pct: None,
                    status: UiText::ConnectingTo {
                        name: kd.name.clone(),
                        addr: ep_label.clone(),
                    },
                    file: UiText::raw("-"),
                });
            }
            let tx_ev = tx.clone();
            let emit = move |event: SyncEvent| {
                let _ = tx_ev.send(WorkerMsg::SyncEvent(event));
            };
            let params = SessionParams {
                identity: &identity,
                hello: &hello,
                storage: &storage,
                expected_device_id: Some(target_id),
                timeout: CONNECT_TIMEOUT,
                control: Some(Arc::clone(&control)),
            };
            let result = match ep {
                PeerEndpoint::Direct(tcp_addr) => connect_and_sync(&params, tcp_addr, emit),
                PeerEndpoint::Relay {
                    addr: relay_addr,
                    device_id: relay_id,
                } => connect_via_relay(
                    &identity,
                    relay_addr,
                    relay_id,
                    target_id,
                    GLOBAL_DISCOVERY_TIMEOUT,
                )
                .and_then(|tcp| connect_and_sync_stream(&params, tcp, relay_addr, emit)),
            };

            match result {
                Ok(hs) => match hs.session_status {
                    SyncSessionStatus::Active { .. } => {
                        connected_once = true;
                        logged_awaiting = false;
                        report_unreachable = true;
                        report_connecting = true;
                        last_error.clear();
                        break;
                    }
                    SyncSessionStatus::AwaitingPairing => {
                        awaiting_pairing = true;
                        if !logged_awaiting {
                            logged_awaiting = true;
                            let _ = tx.send(WorkerMsg::Log(format!(
                                "Peer {} ({}) awaiting device acceptance in Syncthing GUI.",
                                kd.name, ep_label
                            )));
                        }
                    }
                    SyncSessionStatus::ClosedByPeer(reason) => {
                        awaiting_pairing = true;
                        if !logged_awaiting {
                            logged_awaiting = true;
                            let _ = tx.send(WorkerMsg::Log(format!(
                                "Session with {} ({}) closed by peer: {}",
                                kd.name, ep_label, reason
                            )));
                        }
                    }
                },
                Err(e) => {
                    let msg = format!(
                        "Connection attempt to {} ({}) failed: {}",
                        kd.name, ep_label, e
                    );
                    if msg != last_error {
                        let _ = tx.send(WorkerMsg::Log(msg.clone()));
                        last_error = msg;
                    }
                }
            }
        }

        if control.is_stop_requested() {
            break;
        }

        if !connected_once && !awaiting_pairing && report_unreachable {
            report_unreachable = false;
            let _ = tx.send(WorkerMsg::Progress {
                pct: None,
                status: UiText::WaitingForLan,
                file: UiText::raw("-"),
            });
        }

        let retry_delay = if connected_once {
            RETRY_AFTER_SESSION
        } else if awaiting_pairing {
            RETRY_AWAITING_PAIRING
        } else {
            RETRY_AFTER_ERROR
        };
        if !sleep_interruptible(&control, retry_delay) {
            break;
        }
    }
}
