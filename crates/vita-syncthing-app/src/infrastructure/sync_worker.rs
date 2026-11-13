//! Asynchronous BEP/1.0 synchronization worker (`1+2` persistent connections per peer & manual pairing).

use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use syncthing_core::{
    connect_and_sync_with_control, resolve_peer_tcp_addrs, run_discovery_cycle, scan_lan_bep_peers,
    Announce, BepFolder, BepHello, DeviceId, FolderType, FsSyncStorage, SyncControl, SyncEvent,
    SyncSessionStatus, SyncTuning, VitaIdentity, BEP_DEFAULT_PORT, DISCOVERY_BROADCAST_ADDR,
};

use super::worker_slot::spawn_after_previous;
use crate::domain::defaults::{
    CONNECT_TIMEOUT, DISCOVERY_INTERVAL, DISCOVERY_LISTEN, DISCOVERY_SCAN_LISTEN,
    KEEPALIVE_INTERVAL, MAX_PEER_SESSIONS, NO_ADDRESS_POLL, RESCAN_INTERVAL, RETRY_AFTER_ERROR,
    RETRY_AFTER_SESSION, RETRY_AWAITING_PAIRING,
};
use crate::domain::{KnownDevice, ShareConfigEntry, ShareMode, UiText};

/// Stack size of the worker and peer session threads.
const WORKER_STACK_SIZE: usize = 1536 * 1024;
/// Granularity at which a sleeping worker notices a stop request.
const STOP_CHECK_STEP: Duration = Duration::from_millis(200);

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

type SharedDiscoveredAddrs = Arc<Mutex<Vec<(DeviceId, Vec<SocketAddr>)>>>;

fn sleep_interruptible(control: &SyncControl, duration: Duration) -> bool {
    let step = STOP_CHECK_STEP;
    let mut elapsed = Duration::ZERO;
    while elapsed < duration {
        if control.is_stop_requested() {
            return false;
        }
        let rem = (duration - elapsed).min(step);
        std::thread::sleep(rem);
        elapsed += rem;
    }
    !control.is_stop_requested()
}

fn candidate_addrs_for_peer(
    kd: &KnownDevice,
    discovered: &SharedDiscoveredAddrs,
) -> (DeviceId, Vec<SocketAddr>) {
    let mut resolved_id = kd.device_id;
    let mut addrs = Vec::new();
    if let Ok(sock) = kd.address.parse::<SocketAddr>() {
        addrs.push(sock);
    }
    if let Ok(guard) = discovered.lock() {
        for (disc_id, disc_socks) in guard.iter() {
            if kd.device_id.matches_peer(disc_id) {
                resolved_id = *disc_id;
                for &s in disc_socks {
                    if !addrs.contains(&s) {
                        addrs.push(s);
                    }
                }
            }
        }
    }
    (resolved_id, addrs)
}

fn run_peer_session_loop(
    kd: KnownDevice,
    identity: VitaIdentity,
    our_hello: BepHello,
    storage: FsSyncStorage,
    discovered: SharedDiscoveredAddrs,
    control: Arc<SyncControl>,
    tx: Sender<WorkerMsg>,
) {
    let mut logged_awaiting = false;
    let mut logged_no_addr = false;
    let mut report_connecting = true;
    let mut last_error = String::new();
    while !control.is_stop_requested() {
        let (target_id, addrs) = candidate_addrs_for_peer(&kd, &discovered);
        if addrs.is_empty() {
            if !logged_no_addr {
                logged_no_addr = true;
                let _ = tx.send(WorkerMsg::Log(format!(
                    "No address for {} (configured: '{}'). Waiting for LAN discovery...",
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

        for tcp_addr in addrs {
            if control.is_stop_requested() {
                return;
            }
            if report_connecting {
                report_connecting = false;
                let _ = tx.send(WorkerMsg::Progress {
                    pct: None,
                    status: UiText::ConnectingTo {
                        name: kd.name.clone(),
                        addr: tcp_addr.to_string(),
                    },
                    file: UiText::raw("-"),
                });
            }
            let tx_ev = tx.clone();
            match connect_and_sync_with_control(
                &identity,
                tcp_addr,
                Some(target_id),
                &our_hello,
                &storage,
                CONNECT_TIMEOUT,
                Some(Arc::clone(&control)),
                move |event| {
                    let _ = tx_ev.send(WorkerMsg::SyncEvent(event));
                },
            ) {
                Ok(hs) => match hs.session_status {
                    SyncSessionStatus::Active { .. } => {
                        connected_once = true;
                        logged_awaiting = false;
                        last_error.clear();
                        break;
                    }
                    SyncSessionStatus::AwaitingPairing => {
                        awaiting_pairing = true;
                        if !logged_awaiting {
                            logged_awaiting = true;
                            let _ = tx.send(WorkerMsg::Log(format!(
                                "Peer {} ({}) awaiting device acceptance in Syncthing GUI.",
                                kd.name, tcp_addr
                            )));
                        }
                    }
                    SyncSessionStatus::ClosedByPeer(reason) => {
                        awaiting_pairing = true;
                        if !logged_awaiting {
                            logged_awaiting = true;
                            let _ = tx.send(WorkerMsg::Log(format!(
                                "Session with {} ({}) closed by peer: {}",
                                kd.name, tcp_addr, reason
                            )));
                        }
                    }
                },
                Err(e) => {
                    // Log each distinct failure once, so a retry loop does not flood the log
                    // but a new cause is never hidden.
                    let msg = format!(
                        "Connection attempt to {} ({}) failed: {}",
                        kd.name, tcp_addr, e
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

/// Spawns UDP 21027 discovery + concurrent persistent `1+2` TLS 1.3 / BEP sessions for all
/// `approved_devices` so every paired instance (e.g. Linux server and MacOS desktop) stays
/// connected simultaneously while the UI thread keeps running.
pub fn spawn_sync_worker(
    base_data_dir: &str,
    identity: VitaIdentity,
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
        for s in &shares_snapshot {
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
            let device_ids: Vec<DeviceId> = s
                .shared_devices
                .iter()
                .filter_map(|hex| DeviceId::from_hex(hex))
                .collect();
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

        let our_hello = BepHello::new_vita_default();

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
            let peer_id_clone = identity.clone();
            let peer_hello_clone = our_hello.clone();
            let peer_storage_clone = storage.clone();
            let peer_disc_clone = Arc::clone(&discovered_addrs);
            let peer_ctrl_clone = Arc::clone(&worker_control);
            let peer_tx_clone = tx.clone();

            let peer_builder = std::thread::Builder::new()
                .name(format!("bep_peer_{}", idx))
                .stack_size(WORKER_STACK_SIZE);

            match peer_builder.spawn(move || {
                run_peer_session_loop(
                    kd,
                    peer_id_clone,
                    peer_hello_clone,
                    peer_storage_clone,
                    peer_disc_clone,
                    peer_ctrl_clone,
                    peer_tx_clone,
                );
            }) {
                Ok(handle) => peer_handles.push(handle),
                Err(e) => {
                    let _ = tx.send(WorkerMsg::Log(format!(
                        "Cannot start session thread for {}: {}",
                        peer_name, e
                    )));
                }
            }
        }

        // Continuously run UDP 21027 local discovery in the coordinator thread so:
        // 1. All LAN peers (e.g. both Linux and MacOS) receive the Vita's Announce packet.
        // 2. Approved peers with `dynamic` or Short-ID addresses have their IP:port resolved.
        // 3. New LAN peers appear in the `Shares -> Devices` modal even while connected to another peer.
        let mut first_pass = true;
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

            if !sleep_interruptible(&worker_control, DISCOVERY_INTERVAL) {
                break;
            }
        }

        for handle in peer_handles {
            let _ = handle.join();
        }
    });

    (rx, control)
}
