//! Peer endpoint resolution: configured addresses (IP or DNS hostname), LAN UDP discovery,
//! and Syncthing Global Discovery v2 (`tcp://` + `relay://`).

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use syncthing_core::{
    connect_and_sync_stream_with_control, connect_and_sync_with_control, connect_via_relay,
    lookup_global_peer, BepHello, DeviceId, FsSyncStorage, PeerEndpoint, SyncControl, SyncEvent,
    SyncSessionStatus, VitaIdentity,
};

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
/// Configured devices whose session is currently established.
pub type SharedConnectedPeers = Arc<Mutex<Vec<DeviceId>>>;

/// A device is looked up on Global Discovery only when that can help and is possible: it has a
/// full `DeviceId` and no session is established with it. Looking up a connected peer would
/// only tell the discovery server which devices this console talks to.
pub fn needs_global_lookup(kd: &KnownDevice, connected: &[DeviceId]) -> bool {
    !kd.device_id.is_short_placeholder() && !connected.contains(&kd.device_id)
}

fn set_connected(connected: &SharedConnectedPeers, device_id: DeviceId, is_connected: bool) {
    if let Ok(mut guard) = connected.lock() {
        guard.retain(|id| *id != device_id);
        if is_connected {
            guard.push(device_id);
        }
    }
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

/// Collects candidate endpoints for `kd`, ordered with all direct TCP endpoints before relay
/// endpoints so local/WAN direct connections are always preferred over relayed tunnels.
pub fn collect_peer_targets(
    kd: &KnownDevice,
    discovered: &SharedDiscoveredAddrs,
    global: &SharedGlobalEndpoints,
) -> (DeviceId, Vec<PeerEndpoint>) {
    let mut resolved_id = kd.device_id;
    let mut direct = Vec::new();
    let mut relays = Vec::new();

    for sock in resolve_configured_addr(&kd.address) {
        push_unique(&mut direct, PeerEndpoint::Direct(sock));
    }

    if let Ok(guard) = discovered.lock() {
        for (disc_id, disc_socks) in guard.iter() {
            if kd.device_id.matches_peer(disc_id) {
                resolved_id = *disc_id;
                for &s in disc_socks {
                    push_unique(&mut direct, PeerEndpoint::Direct(s));
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
                        PeerEndpoint::Direct(_) => push_unique(&mut direct, ep.clone()),
                        PeerEndpoint::Relay { .. } => push_unique(&mut relays, ep.clone()),
                    }
                }
            }
        }
    }

    direct.extend(relays);
    (resolved_id, direct)
}

/// Queries Global Discovery v2 for every configured peer that needs it (see
/// [`needs_global_lookup`]) and updates the shared endpoint cache.
pub fn refresh_global_endpoints(
    approved_devices: &[KnownDevice],
    global: &SharedGlobalEndpoints,
    connected: &SharedConnectedPeers,
    control: &SyncControl,
    tx: &Sender<WorkerMsg>,
    last_errors: &mut Vec<(DeviceId, String)>,
) {
    for kd in approved_devices {
        if control.is_stop_requested() {
            return;
        }
        let connected_now = connected.lock().map(|g| g.clone()).unwrap_or_default();
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
                let msg = format!(
                    "Global discovery lookup for {} failed: {}",
                    kd.name, e
                );
                let prev = last_errors
                    .iter_mut()
                    .find(|(id, _)| *id == kd.device_id);
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

fn sleep_interruptible(control: &SyncControl, duration: Duration) -> bool {
    let mut elapsed = Duration::ZERO;
    while elapsed < duration {
        if control.is_stop_requested() {
            return false;
        }
        let rem = (duration - elapsed).min(STOP_CHECK_STEP);
        std::thread::sleep(rem);
        elapsed += rem;
    }
    !control.is_stop_requested()
}

fn format_endpoint_label(ep: &PeerEndpoint) -> String {
    match ep {
        PeerEndpoint::Direct(addr) => addr.to_string(),
        PeerEndpoint::Relay { addr, .. } => format!("relay://{addr}"),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_peer_session_loop(
    kd: KnownDevice,
    identity: VitaIdentity,
    our_hello: BepHello,
    storage: FsSyncStorage,
    discovered: SharedDiscoveredAddrs,
    global: SharedGlobalEndpoints,
    connected: SharedConnectedPeers,
    control: Arc<SyncControl>,
    tx: Sender<WorkerMsg>,
) {
    let mut logged_awaiting = false;
    let mut logged_no_addr = false;
    let mut report_connecting = true;
    let mut report_unreachable = true;
    let mut last_error = String::new();
    while !control.is_stop_requested() {
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

        let mut connected_once = false;
        let mut awaiting_pairing = false;

        for ep in targets {
            if control.is_stop_requested() {
                return;
            }
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
            let connected_ev = Arc::clone(&connected);
            let configured_id = kd.device_id;
            let emit = move |event: SyncEvent| {
                if matches!(event, SyncEvent::PeerConnected { .. }) {
                    set_connected(&connected_ev, configured_id, true);
                }
                let _ = tx_ev.send(WorkerMsg::SyncEvent(event));
            };
            let result = match ep {
                PeerEndpoint::Direct(tcp_addr) => connect_and_sync_with_control(
                    &identity,
                    tcp_addr,
                    Some(target_id),
                    &our_hello,
                    &storage,
                    CONNECT_TIMEOUT,
                    Some(Arc::clone(&control)),
                    emit,
                ),
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
                .and_then(|tcp| {
                    connect_and_sync_stream_with_control(
                        &identity,
                        tcp,
                        relay_addr,
                        Some(target_id),
                        &our_hello,
                        &storage,
                        CONNECT_TIMEOUT,
                        Some(Arc::clone(&control)),
                        emit,
                    )
                }),
            };

            set_connected(&connected, kd.device_id, false);

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

