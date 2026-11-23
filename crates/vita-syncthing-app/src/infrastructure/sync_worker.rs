//! Asynchronous BEP/1.0 synchronization worker (`1+2` persistent connections per peer & manual pairing).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use syncthing_core::{
    resolve_peer_tcp_addrs, run_discovery_cycle, scan_lan_bep_peers_interruptible, Announce,
    BepFolder, BepHello, DeviceId, DeviceIdentity, FolderType, FsSyncStorage, SyncControl,
    SyncEvent, SyncTuning, BEP_DEFAULT_PORT, DISCOVERY_BROADCAST_ADDR,
};

use super::global_announcer::{run_global_announcer, GlobalAnnouncerContext};
use super::incoming_listener::{bind_bep_listener, run_tcp_listener, IncomingListenerContext};
use super::peer_endpoints::{
    run_global_discovery_loop, run_peer_session_loop, sleep_unless_stopped, PeerSessionShared,
    SharedDiscoveredAddrs, SharedGlobalEndpoints,
};
use super::relay_listener::{run_relay_listener, RelayListenerContext, SharedRelayUri};
use super::session_guard::{ConnectionSlots, SessionClaims, UnknownDeviceLog};
use super::worker_slot::spawn_after_previous;
use super::ConfigRepository;
use crate::domain::defaults::{
    BEP_LISTEN_PORT, CLIENT_NAME, DEVICE_NAME, DISCOVERY_INTERVAL, DISCOVERY_LISTEN,
    KEEPALIVE_INTERVAL, MAX_INCOMING_HANDSHAKES, MAX_PEER_SESSIONS, MAX_UNKNOWN_DEVICE_REPORTS,
    RESCAN_INTERVAL,
};
use crate::domain::{KnownDevice, ShareConfigEntry, ShareMode, UiText};

/// Stack size of threads performing TLS 1.3 / BEP I/O.
pub(crate) const WORKER_STACK_SIZE: usize = 1536 * 1024;
/// Stack size of the lightweight non-blocking TCP accept loop (no TLS on this thread).
const LISTENER_STACK_SIZE: usize = 256 * 1024;

static PROCESS_INSTANCE_ID: OnceLock<i64> = OnceLock::new();

pub fn process_instance_id() -> i64 {
    *PROCESS_INSTANCE_ID.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_nanos() as i64) ^ 0x5649_5441_0000_0006)
            .unwrap_or(0x5649_5441_0000_0006)
    })
}

/// Messages sent by the BEP network threads to the UI thread.
pub enum WorkerMsg {
    Log(String),
    Progress {
        pct: Option<u32>,
        status: UiText,
        file: UiText,
    },
    DeviceDiscovered {
        device_id: DeviceId,
        name: String,
        address: String,
    },
    DeviceOffered {
        device_id: DeviceId,
        address: String,
        via_relay: bool,
    },
    SyncEvent(SyncEvent),
    InboundSyncEvent(SyncEvent),
    Finished {
        summary: UiText,
        allow_auto_reconnect: bool,
    },
}

/// Builds the TCP addresses advertised in LAN Local Discovery (UDP 21027).
///
/// Always advertises an explicit non-loopback local IPv4 address (`tcp://<local_ip>:<port>`)
/// rather than `tcp://0.0.0.0:<port>`: when a peer runs inside Docker NAT, `0.0.0.0` causes
/// Syncthing to substitute the Docker bridge gateway IP (e.g. `172.24.0.1`) and dial itself.
pub fn lan_announce_addresses(
    listen_port: Option<u16>,
    local_ip: Option<std::net::Ipv4Addr>,
) -> Vec<String> {
    let Some(port) = listen_port.filter(|&p| p != 0) else {
        return Vec::new();
    };
    let Some(ip) = local_ip.filter(|ip| !ip.is_unspecified() && !ip.is_loopback()) else {
        return Vec::new();
    };
    vec![format!("tcp://{ip}:{port}")]
}

/// Selects the TCP port to advertise on Global Discovery v2 (if any).
///
/// Behind home NAT without an explicit UPnP/NAT-PMP external port mapping, advertising
/// `tcp://0.0.0.0:22000` causes Global Discovery to publish `<public_ip>:22000`, which is
/// unreachable or hits another port-forwarded device on the same router. Only advertise a TCP
/// port on Global Discovery when a real external port mapping exists; otherwise stay reachable
/// over WAN via the relay URI.
pub fn global_announce_port(
    listen_port: Option<u16>,
    mapped_external_port: Option<u16>,
) -> Option<u16> {
    listen_port.filter(|&p| p != 0)?;
    mapped_external_port.filter(|&p| p != 0)
}

/// Records newly discovered LAN peers in `discovered_addrs` and notifies the UI thread.
pub fn publish_discovered_peers(
    peers: &[(SocketAddr, Announce)],
    discovered_addrs: &SharedDiscoveredAddrs,
    tx: &Sender<WorkerMsg>,
) {
    let mut seen_ids = Vec::new();
    for (udp_src, peer) in peers {
        if seen_ids.contains(&peer.id) {
            continue;
        }
        seen_ids.push(peer.id);

        let tcp_targets = resolve_peer_tcp_addrs(*udp_src, &peer.addresses);
        let best_tcp = tcp_targets
            .first()
            .copied()
            .unwrap_or_else(|| SocketAddr::new(udp_src.ip(), BEP_DEFAULT_PORT));

        if let Ok(mut guard) = discovered_addrs.lock() {
            if let Some(slot) = guard.iter_mut().find(|(id, _)| *id == peer.id) {
                slot.1 = tcp_targets;
            } else {
                guard.push((peer.id, tcp_targets));
            }
        }

        let _ = tx.send(WorkerMsg::DeviceDiscovered {
            device_id: peer.id,
            name: "LAN Peer".to_owned(),
            address: best_tcp.to_string(),
        });
    }
}

/// Waits up to `duration` before the next discovery cycle. Returns early when the UI asks for a
/// LAN scan (`SyncControl::request_discovery`); `false` if a stop was requested.
fn wait_for_next_discovery(control: &SyncControl, duration: Duration) -> bool {
    sleep_unless_stopped(control, duration, || control.take_discovery_request())
}

/// Spawns the full BEP/1.0 worker:
/// - Incoming TCP listener (`0.0.0.0:22000` with ephemeral fallback) accepting `approved_devices`
///   and emitting [`WorkerMsg::DeviceOffered`] for unknown peers;
/// - Permanent Relay Protocol v1 listener & Global Discovery v2 announcer (active even with 0
///   paired devices so remote peers can reach this PS Vita immediately);
/// - Continuous UDP 21027 Local Discovery broadcasting our listen address;
/// - Outgoing persistent `1+2` session loops for `dial_peers`, deduplicated against inbound
///   sessions via [`SessionClaims`].
pub fn spawn_sync_worker(
    base_data_dir: &str,
    identity: DeviceIdentity,
    shares_snapshot: Vec<ShareConfigEntry>,
    approved_devices: Vec<KnownDevice>,
    dial_peers: Vec<KnownDevice>,
    offered_folder_ids: Vec<String>,
) -> (Receiver<WorkerMsg>, Arc<SyncControl>) {
    let (tx, rx): (Sender<WorkerMsg>, Receiver<WorkerMsg>) = mpsc::channel();
    let control = SyncControl::with_tuning(SyncTuning {
        rescan_interval: RESCAN_INTERVAL,
        keepalive_interval: KEEPALIVE_INTERVAL,
    });
    let worker_control = Arc::clone(&control);
    let sync_root = format!("{}/sync", base_data_dir);
    let worker_base_dir = base_data_dir.to_owned();

    let wait_tx = tx.clone();
    let on_wait = move || {
        let _ = wait_tx.send(WorkerMsg::Log(
            "Waiting for the previous sync session to stop...".to_owned(),
        ));
    };

    // The worker only starts once the previous one has fully stopped (see `worker_slot`).
    let _ = spawn_after_previous("bep_worker", WORKER_STACK_SIZE, on_wait, move || {
        if worker_control.is_stop_requested() {
            return;
        }
        let mut storage = FsSyncStorage::new(&sync_root);
        for d in &approved_devices {
            storage.add_known_device(d.device_id);
        }
        let repo = ConfigRepository::new(&worker_base_dir);
        for s in &shares_snapshot {
            // Defence in depth (the folder browser refuses such a target): never synchronize
            // the app's own data, whatever `shares.cfg` says.
            if repo.is_protected_share_path(&s.target_path) {
                let _ = tx.send(WorkerMsg::Log(format!(
                    "Share {} not synchronized: {} holds the app's own data.",
                    s.folder_id, s.target_path
                )));
                continue;
            }
            storage.set_folder_path(&s.folder_id, &s.target_path);
            storage.set_folder_paused(&s.folder_id, !s.enabled);
            // A read-only share is Syncthing's "Receive Only"; a send-only share never pulls
            // remote changes.
            storage.set_folder_type(
                &s.folder_id,
                match s.mode {
                    ShareMode::ReadWrite => FolderType::SendReceive,
                    ShareMode::ReadOnly => FolderType::ReceiveOnly,
                    ShareMode::SendOnly => FolderType::SendOnly,
                },
            );
            // Authorization: a folder is only synchronized with the devices linked to its share.
            let device_ids = s.shared_devices.clone();
            storage.set_folder_devices(&s.folder_id, device_ids.clone());
            storage.set_folder_label(&s.folder_id, &s.label);
            if s.enabled
                && !device_ids.is_empty()
                && offered_folder_ids.iter().any(|fid| fid == &s.folder_id)
            {
                storage.add_local_folder(BepFolder {
                    id: s.folder_id.clone(),
                    label: s.label.clone(),
                    device_ids,
                    ..Default::default()
                });
            }
        }

        let our_hello = BepHello::new(DEVICE_NAME, CLIENT_NAME, env!("CARGO_PKG_VERSION"));

        // Bind incoming BEP/1.0 TCP listener so peers on the LAN (or via port-forwarding) can
        // initiate connections to the Vita.
        let (tcp_listener, listen_port) = match bind_bep_listener(BEP_LISTEN_PORT) {
            Ok((listener, port)) => {
                let _ = tx.send(WorkerMsg::Log(format!(
                    "Listening for incoming BEP/1.0 connections on 0.0.0.0:{port}"
                )));
                (Some(listener), Some(port))
            }
            Err(e) => {
                let _ = tx.send(WorkerMsg::Log(format!(
                    "Cannot bind BEP listener ({e}): outgoing/relay reachability only"
                )));
                (None, None)
            }
        };

        let announce = Announce {
            id: identity.device_id,
            addresses: lan_announce_addresses(listen_port, syncthing_core::detect_local_ipv4()),
            instance_id: process_instance_id(),
        };

        let discovered_addrs: SharedDiscoveredAddrs = Arc::new(Mutex::new(Vec::new()));
        let global_endpoints: SharedGlobalEndpoints = Arc::new(Mutex::new(Vec::new()));
        let claims = Arc::new(SessionClaims::new(MAX_PEER_SESSIONS));
        let secondary_inbox = Arc::new(syncthing_core::SecondaryInbox::new());
        let tcp_slots = Arc::new(ConnectionSlots::new(MAX_INCOMING_HANDSHAKES));
        let unknown_log = Arc::new(UnknownDeviceLog::new(MAX_UNKNOWN_DEVICE_REPORTS));
        let shared_relay_uri: SharedRelayUri = Arc::new(Mutex::new(None));
        let initial_lan_done = Arc::new(AtomicBool::new(false));
        let mut child_handles = Vec::new();

        // 1. Incoming TCP listener thread (active even with 0 paired devices).
        if let Some(listener) = tcp_listener {
            let listener_ctx = IncomingListenerContext {
                identity: identity.clone(),
                hello: our_hello.clone(),
                storage: storage.clone(),
                approved_devices: approved_devices.clone(),
                claims: Arc::clone(&claims),
                secondary_inbox: Arc::clone(&secondary_inbox),
                slots: Arc::clone(&tcp_slots),
                unknown_log: Arc::clone(&unknown_log),
                control: Arc::clone(&worker_control),
                tx: tx.clone(),
            };
            match std::thread::Builder::new()
                .name("bep_listener".to_owned())
                .stack_size(LISTENER_STACK_SIZE)
                .spawn(move || run_tcp_listener(listener, listener_ctx))
            {
                Ok(handle) => child_handles.push(handle),
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Log(format!(
                        "Cannot start incoming TCP listener thread: {e}"
                    )));
                }
            }
        }

        // 2. Permanent Relay Protocol v1 listener thread (active even with 0 paired devices).
        {
            let relay_ctx = RelayListenerContext {
                identity: identity.clone(),
                hello: our_hello.clone(),
                storage: storage.clone(),
                approved_devices: approved_devices.clone(),
                claims: Arc::clone(&claims),
                unknown_log: Arc::clone(&unknown_log),
                shared_relay_uri: Arc::clone(&shared_relay_uri),
                control: Arc::clone(&worker_control),
                tx: tx.clone(),
            };
            match std::thread::Builder::new()
                .name("bep_relay".to_owned())
                .stack_size(WORKER_STACK_SIZE)
                .spawn(move || run_relay_listener(relay_ctx))
            {
                Ok(handle) => child_handles.push(handle),
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Log(format!(
                        "Cannot start relay listener thread: {e}"
                    )));
                }
            }
        }

        // 3. Global Discovery v2 announcer thread (active even with 0 paired devices).
        {
            let announce_ctx = GlobalAnnouncerContext {
                identity: identity.clone(),
                listen_port: global_announce_port(listen_port, None),
                shared_relay_uri: Arc::clone(&shared_relay_uri),
                control: Arc::clone(&worker_control),
                tx: tx.clone(),
            };
            match std::thread::Builder::new()
                .name("bep_announce".to_owned())
                .stack_size(WORKER_STACK_SIZE)
                .spawn(move || run_global_announcer(announce_ctx))
            {
                Ok(handle) => child_handles.push(handle),
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Log(format!(
                        "Cannot start Global Discovery announcer thread: {e}"
                    )));
                }
            }
        }

        // 4. Outgoing persistent `1+2` session workers for `dial_peers` (up to `MAX_PEER_SESSIONS`).
        if !dial_peers.is_empty() {
            let peer_list: Vec<String> = dial_peers
                .iter()
                .take(MAX_PEER_SESSIONS)
                .map(|kd| format!("{} [{}] @ {}", kd.name, kd.device_id.short(), kd.address))
                .collect();
            let _ = tx.send(WorkerMsg::Log(format!(
                "Linked peers: {}",
                peer_list.join(", ")
            )));

            if dial_peers.len() > MAX_PEER_SESSIONS {
                let _ = tx.send(WorkerMsg::Log(format!(
                    "{} devices are linked: only the first {} are synchronized.",
                    dial_peers.len(),
                    MAX_PEER_SESSIONS
                )));
            }

            for (idx, kd) in dial_peers
                .iter()
                .take(MAX_PEER_SESSIONS)
                .cloned()
                .enumerate()
            {
                let peer_name = kd.name.clone();
                let shared = PeerSessionShared {
                    identity: identity.clone(),
                    hello: our_hello.clone(),
                    storage: storage.clone(),
                    discovered: Arc::clone(&discovered_addrs),
                    global: Arc::clone(&global_endpoints),
                    claims: Arc::clone(&claims),
                    initial_lan_done: Arc::clone(&initial_lan_done),
                    control: Arc::clone(&worker_control),
                    tx: tx.clone(),
                };
                let peer_builder = std::thread::Builder::new()
                    .name(format!("bep_peer_{}", idx))
                    .stack_size(WORKER_STACK_SIZE);

                match peer_builder.spawn(move || run_peer_session_loop(kd, shared)) {
                    Ok(handle) => child_handles.push(handle),
                    Err(e) => {
                        let _ = tx.send(WorkerMsg::Log(format!(
                            "Cannot start session thread for {}: {}",
                            peer_name, e
                        )));
                    }
                }
            }

            // Start Global Discovery v2 lookups immediately in parallel with UDP Local Discovery.
            {
                let global_peers = dial_peers.clone();
                let global_ref = Arc::clone(&global_endpoints);
                let global_claims = Arc::clone(&claims);
                let global_control = Arc::clone(&worker_control);
                let global_tx = tx.clone();
                match std::thread::Builder::new()
                    .name("bep_global".to_owned())
                    .stack_size(WORKER_STACK_SIZE)
                    .spawn(move || {
                        run_global_discovery_loop(
                            global_peers,
                            global_ref,
                            global_claims,
                            global_control,
                            global_tx,
                        )
                    }) {
                    Ok(handle) => child_handles.push(handle),
                    Err(e) => {
                        let _ = tx.send(WorkerMsg::Log(format!(
                            "Cannot start Global Discovery lookup thread: {e}"
                        )));
                    }
                }
            }
        }

        // 5. Continuous UDP 21027 Local Discovery (plus first-pass interruptible TCP subnet fallback).
        let mut first_pass = true;
        while !worker_control.is_stop_requested() {
            let discovered_peers =
                match run_discovery_cycle(&announce, DISCOVERY_BROADCAST_ADDR, DISCOVERY_LISTEN) {
                    Ok(res) => res.discovered_peers,
                    Err(_) => Vec::new(),
                };

            publish_discovered_peers(&discovered_peers, &discovered_addrs, &tx);

            if first_pass {
                initial_lan_done.store(true, Ordering::Release);
                first_pass = false;

                if !dial_peers.is_empty() && !worker_control.is_stop_requested() {
                    let established = claims.established_ids();
                    let missing_dial = dial_peers.iter().any(|kd| {
                        !discovered_peers
                            .iter()
                            .any(|(_, p)| kd.device_id.matches_peer(&p.id))
                            && !established.iter().any(|id| id.matches_peer(&kd.device_id))
                    });
                    if missing_dial {
                        let mut scanned = Vec::new();
                        for (addr, peer) in
                            scan_lan_bep_peers_interruptible(identity.device_id, || {
                                worker_control.is_stop_requested()
                            })
                        {
                            if !discovered_peers.iter().any(|(_, p)| p.id == peer.id)
                                && !scanned
                                    .iter()
                                    .any(|(_, p): &(SocketAddr, Announce)| p.id == peer.id)
                            {
                                scanned.push((addr, peer));
                            }
                        }
                        if !scanned.is_empty() {
                            publish_discovered_peers(&scanned, &discovered_addrs, &tx);
                        }
                    }
                }
            }

            if !wait_for_next_discovery(&worker_control, DISCOVERY_INTERVAL) {
                break;
            }
        }

        for handle in child_handles {
            let _ = handle.join();
        }
    });

    (rx, control)
}
