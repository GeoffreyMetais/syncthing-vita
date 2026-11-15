//! Asynchronous BEP/1.0 synchronization worker (`1+2` persistent connections per peer & manual pairing).

use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use syncthing_core::{
    resolve_peer_tcp_addrs, run_discovery_cycle, scan_lan_bep_peers, Announce, BepFolder, BepHello,
    DeviceId, DeviceIdentity, FolderType, FsSyncStorage, SyncControl, SyncEvent, SyncTuning,
    BEP_DEFAULT_PORT, DISCOVERY_BROADCAST_ADDR,
};

use super::peer_endpoints::{
    refresh_global_endpoints, run_peer_session_loop, sleep_unless_stopped, PeerSessionShared,
    SharedConnectedPeers, SharedDiscoveredAddrs, SharedGlobalEndpoints,
};
use super::worker_slot::spawn_after_previous;
use super::ConfigRepository;
use crate::domain::defaults::{
    CLIENT_NAME, DEVICE_NAME, DISCOVERY_INTERVAL, DISCOVERY_LISTEN, DISCOVERY_SCAN_LISTEN,
    GLOBAL_DISCOVERY_INTERVAL, KEEPALIVE_INTERVAL, MAX_PEER_SESSIONS, RESCAN_INTERVAL,
};
use crate::domain::{KnownDevice, ShareConfigEntry, ShareMode, UiText};

/// Stack size of the worker and peer session threads.
const WORKER_STACK_SIZE: usize = 1536 * 1024;

static PROCESS_INSTANCE_ID: OnceLock<i64> = OnceLock::new();

fn process_instance_id() -> i64 {
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
    SyncEvent(SyncEvent),
    Finished {
        summary: UiText,
        allow_auto_reconnect: bool,
    },
}

/// Waits up to `duration` before the next discovery cycle. Returns early when the UI asks for a
/// LAN scan (`SyncControl::request_discovery`); `false` if a stop was requested.
fn wait_for_next_discovery(control: &SyncControl, duration: Duration) -> bool {
    sleep_unless_stopped(control, duration, || control.take_discovery_request())
}

/// Spawns UDP 21027 discovery + concurrent persistent `1+2` TLS 1.3 / BEP sessions for all
/// `approved_devices` so every paired instance (e.g. Linux server and MacOS desktop) stays
/// connected simultaneously while the UI thread keeps running.
pub fn spawn_sync_worker(
    base_data_dir: &str,
    identity: DeviceIdentity,
    shares_snapshot: Vec<ShareConfigEntry>,
    approved_devices: Vec<KnownDevice>,
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
            // A read-only share is Syncthing's "Receive Only": its local changes are never sent.
            storage.set_folder_type(
                &s.folder_id,
                match s.mode {
                    ShareMode::ReadWrite => FolderType::SendReceive,
                    ShareMode::ReadOnly => FolderType::ReceiveOnly,
                },
            );
            // Authorization: a folder is only synchronized with the devices linked to its share.
            let device_ids = s.shared_devices.clone();
            storage.set_folder_devices(&s.folder_id, device_ids.clone());
            if s.enabled
                && !device_ids.is_empty()
                && offered_folder_ids.iter().any(|fid| fid == &s.folder_id)
            {
                storage.add_local_folder(BepFolder {
                    id: s.folder_id.clone(),
                    label: s.label.clone(),
                    device_ids,
                });
            }
        }

        // The Vita never listens on TCP 22000 (it always dials out), so it advertises no address.
        // Advertising `tcp://0.0.0.0:22000` made peers dial back the UDP source IP, which behind
        // Docker NAT is the bridge gateway (e.g. `172.24.0.1`) and ends as "connected to self".
        // Syncthing ignores announcements without addresses, so peers never try to dial the Vita.
        let announce = Announce {
            id: identity.device_id,
            addresses: Vec::new(),
            instance_id: process_instance_id(),
        };

        let our_hello = BepHello::new(DEVICE_NAME, CLIENT_NAME, env!("CARGO_PKG_VERSION"));

        // If no peer has been manually linked/added yet, run passive LAN discovery only (never dial TCP 22000).
        if approved_devices.is_empty() {
            let _ = tx.send(WorkerMsg::Progress {
                pct: None,
                status: UiText::ScanningLan,
                file: UiText::raw("-"),
            });
            match run_discovery_cycle(&announce, DISCOVERY_BROADCAST_ADDR, DISCOVERY_SCAN_LISTEN) {
                Ok(res) => {
                    let mut count = 0usize;
                    let mut seen_ids = Vec::new();
                    for (udp_src, peer) in &res.discovered_peers {
                        if seen_ids.contains(&peer.id) {
                            continue;
                        }
                        seen_ids.push(peer.id);
                        count += 1;
                        let best_tcp = resolve_peer_tcp_addrs(*udp_src, &peer.addresses)
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| SocketAddr::new(udp_src.ip(), BEP_DEFAULT_PORT));
                        let _ = tx.send(WorkerMsg::DeviceDiscovered {
                            device_id: peer.id,
                            name: "LAN Peer".to_owned(),
                            address: best_tcp.to_string(),
                        });
                    }
                    let (summary, log) = if count > 0 {
                        (
                            UiText::FoundLanPeers { count },
                            format!("Found {} LAN peer(s).", count),
                        )
                    } else {
                        (
                            UiText::NoPeerAddDevice,
                            "No LAN peer found and no device linked.".to_owned(),
                        )
                    };
                    let _ = tx.send(WorkerMsg::Log(log));
                    let _ = tx.send(WorkerMsg::Finished {
                        summary,
                        allow_auto_reconnect: false,
                    });
                }
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Log(format!("UDP discovery error: {}", e)));
                    let _ = tx.send(WorkerMsg::Finished {
                        summary: UiText::DiscoveryError {
                            error: e.to_string(),
                        },
                        allow_auto_reconnect: false,
                    });
                }
            }
            return;
        }

        let discovered_addrs: SharedDiscoveredAddrs = Arc::new(Mutex::new(Vec::new()));
        let global_endpoints: SharedGlobalEndpoints = Arc::new(Mutex::new(Vec::new()));
        let connected_peers: SharedConnectedPeers = Arc::new(Mutex::new(Vec::new()));
        let mut peer_handles = Vec::new();

        let peer_list: Vec<String> = approved_devices
            .iter()
            .take(MAX_PEER_SESSIONS)
            .map(|kd| format!("{} [{}] @ {}", kd.name, kd.device_id.short(), kd.address))
            .collect();
        let _ = tx.send(WorkerMsg::Log(format!(
            "Linked peers: {}",
            peer_list.join(", ")
        )));

        if approved_devices.len() > MAX_PEER_SESSIONS {
            let _ = tx.send(WorkerMsg::Log(format!(
                "{} devices are linked: only the first {} are synchronized.",
                approved_devices.len(),
                MAX_PEER_SESSIONS
            )));
        }

        // Spawn a concurrent persistent `1+2` session worker for each user-approved device (up to `MAX_PEER_SESSIONS`)
        for (idx, kd) in approved_devices
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
                connected: Arc::clone(&connected_peers),
                control: Arc::clone(&worker_control),
                tx: tx.clone(),
            };
            let peer_builder = std::thread::Builder::new()
                .name(format!("bep_peer_{}", idx))
                .stack_size(WORKER_STACK_SIZE);

            match peer_builder.spawn(move || run_peer_session_loop(kd, shared)) {
                Ok(handle) => peer_handles.push(handle),
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Log(format!(
                        "Cannot start session thread for {}: {}",
                        peer_name, e
                    )));
                }
            }
        }

        // Continuously run UDP 21027 local discovery + periodic Global Discovery v2 in the coordinator thread so:
        // 1. All LAN peers receive the Vita's Announce packet.
        // 2. Approved peers with `dynamic` or Short-ID addresses have their IP:port resolved.
        // 3. Peers outside the LAN have their WAN direct & relay endpoints resolved via Global Discovery v2.
        let mut first_pass = true;
        let mut last_global_lookup: Option<Instant> = None;
        let mut global_errors = Vec::new();
        while !worker_control.is_stop_requested() {
            if let Ok(mut res) =
                run_discovery_cycle(&announce, DISCOVERY_BROADCAST_ADDR, DISCOVERY_LISTEN)
            {
                let needs_dynamic_resolution = approved_devices
                    .iter()
                    .any(|kd| kd.address == "dynamic" || kd.device_id.is_short_placeholder());
                if first_pass && res.discovered_peers.is_empty() && needs_dynamic_resolution {
                    res.discovered_peers = scan_lan_bep_peers(identity.device_id);
                }

                let mut seen_ids = Vec::new();
                for (udp_src, peer) in &res.discovered_peers {
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
            first_pass = false;

            if last_global_lookup.is_none_or(|t| t.elapsed() >= GLOBAL_DISCOVERY_INTERVAL)
                && !worker_control.is_stop_requested()
            {
                refresh_global_endpoints(
                    &approved_devices,
                    &global_endpoints,
                    &connected_peers,
                    &worker_control,
                    &tx,
                    &mut global_errors,
                );
                last_global_lookup = Some(Instant::now());
            }

            if !wait_for_next_discovery(&worker_control, DISCOVERY_INTERVAL) {
                break;
            }
        }

        for handle in peer_handles {
            let _ = handle.join();
        }
    });

    (rx, control)
}
