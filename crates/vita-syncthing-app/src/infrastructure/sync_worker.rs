//! Asynchronous BEP/1.0 synchronization worker thread.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use syncthing_core::{
    connect_and_sync, resolve_peer_tcp_addrs, run_discovery_cycle, Announce, BepFolder, BepHello,
    DeviceId, FsSyncStorage, SyncEvent, SyncSessionStatus, VitaIdentity, BEP_DEFAULT_PORT,
    DISCOVERY_BROADCAST_ADDR,
};

use crate::domain::{KnownDevice, ShareConfigEntry};

/// Messages sent by the BEP network thread to the 60 FPS UI thread.
pub enum WorkerMsg {
    Log(String),
    Progress {
        pct: u32,
        status: String,
        file: String,
    },
    DeviceDiscovered {
        device_id: DeviceId,
        name: String,
        address: String,
    },
    SyncEvent(SyncEvent),
    Finished(String),
}

/// Spawns a complete UDP discovery + TLS 1.3 / BEP synchronization cycle in a dedicated thread
/// (`2 MB` stack) so that the interface (gamepad, touch, OLED saver) remains smooth at 60 FPS.
pub fn spawn_sync_worker(
    base_data_dir: &str,
    identity: VitaIdentity,
    shares_snapshot: Vec<ShareConfigEntry>,
    known_devices: Vec<KnownDevice>,
) -> Receiver<WorkerMsg> {
    let (tx, rx): (Sender<WorkerMsg>, Receiver<WorkerMsg>) = mpsc::channel();
    let sync_root = format!("{}/sync", base_data_dir);

    let builder = std::thread::Builder::new()
        .name("bep_worker".to_owned())
        .stack_size(2 * 1024 * 1024);

    let _ = builder.spawn(move || {
        let _ = tx.send(WorkerMsg::Progress {
            pct: 10,
            status: "LAN discovery (UDP 21027 + TCP 22000)...".to_owned(),
            file: "-".to_owned(),
        });

        let mut storage = FsSyncStorage::new(&sync_root);
        for s in &shares_snapshot {
            storage.set_folder_path(&s.folder_id, &s.target_path);
            storage.set_folder_paused(&s.folder_id, !s.enabled);
            if s.enabled {
                let device_ids: Vec<DeviceId> = s
                    .shared_devices
                    .iter()
                    .filter_map(|hex| DeviceId::from_hex(hex))
                    .collect();
                storage.add_local_folder(BepFolder {
                    id: s.folder_id.clone(),
                    label: s.label.clone(),
                    device_ids,
                });
            }
        }

        let dyn_instance_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_nanos() as i64) ^ 0x5649_5441_0000_0006)
            .unwrap_or(0x5649_5441_0000_0006);

        let announce = Announce {
            id: identity.device_id,
            addresses: vec![format!("tcp://0.0.0.0:{}", BEP_DEFAULT_PORT)],
            instance_id: dyn_instance_id,
        };

        let our_hello = BepHello::new_vita_default();

        // Fast-path: if we already have known devices linked to one of our shares, connect directly first
        // so the BEP/1.0 connection stays persistent without a 2-second discovery gap between cycles.
        for kd in &known_devices {
            let kd_hex = kd.device_id.to_hex();
            let is_linked = shares_snapshot
                .iter()
                .any(|s| s.enabled && s.shared_devices.contains(&kd_hex));
            if !is_linked {
                continue;
            }
            if let Ok(tcp_addr) = kd.address.parse::<std::net::SocketAddr>() {
                let _ = tx.send(WorkerMsg::Progress {
                    pct: 25,
                    status: format!("Connecting TLS 1.3 to {}...", tcp_addr),
                    file: "-".to_owned(),
                });
                let tx_ev = tx.clone();
                if let Ok(hs) = connect_and_sync(
                    &identity,
                    tcp_addr,
                    Some(kd.device_id),
                    &our_hello,
                    &storage,
                    Duration::from_secs(4),
                    move |event| {
                        let _ = tx_ev.send(WorkerMsg::SyncEvent(event));
                    },
                ) {
                    if let SyncSessionStatus::Active {
                        files_up_to_date,
                        files_downloaded,
                        bytes_downloaded,
                        blocks_uploaded,
                        bytes_uploaded,
                        ..
                    } = hs.session_status
                    {
                        let summary = format!(
                            "Sync OK: {} up-to-date | {} DL ({} KB) | {} UL ({} B)",
                            files_up_to_date,
                            files_downloaded,
                            bytes_downloaded / 1024,
                            blocks_uploaded,
                            bytes_uploaded
                        );
                        let _ = tx.send(WorkerMsg::Finished(summary));
                        return;
                    }
                }
            }
        }

        match run_discovery_cycle(&announce, DISCOVERY_BROADCAST_ADDR, Duration::from_secs(2)) {
            Ok(mut res) => {
                // Include any previously saved devices from `devices.cfg` if not in discovered_peers
                for kd in &known_devices {
                    if !res.discovered_peers.iter().any(|(_, a)| a.id == kd.device_id) {
                        if let Ok(sock_addr) = kd.address.parse::<std::net::SocketAddr>() {
                            res.discovered_peers.push((
                                sock_addr,
                                Announce {
                                    id: kd.device_id,
                                    addresses: vec![format!(
                                        "tcp://{}:{}",
                                        sock_addr.ip(),
                                        BEP_DEFAULT_PORT
                                    )],
                                    instance_id: 0,
                                },
                            ));
                        }
                    }
                }

                if res.discovered_peers.is_empty() {
                    let _ = tx.send(WorkerMsg::Finished(
                        "No peers detected on LAN (press Cross to retry).".to_owned(),
                    ));
                    return;
                }

                // Prioritize peers that are explicitly linked to one of our shares
                res.discovered_peers.sort_by_key(|(_, peer)| {
                    let p_hex = peer.id.to_hex();
                    let is_linked = shares_snapshot
                        .iter()
                        .any(|s| s.shared_devices.contains(&p_hex));
                    !is_linked
                });

                let mut seen_ids = Vec::new();
                let mut last_summary: Option<String> = None;

                for (udp_src, peer) in &res.discovered_peers {
                    if seen_ids.contains(&peer.id) {
                        continue;
                    }
                    seen_ids.push(peer.id);

                    let _ = tx.send(WorkerMsg::DeviceDiscovered {
                        device_id: peer.id,
                        name: "LAN Peer".to_owned(),
                        address: udp_src.to_string(),
                    });

                    let _ = tx.send(WorkerMsg::Log(format!(
                        "Discovered peer: {} (UDP {})",
                        peer.id.short(),
                        udp_src
                    )));

                    let tcp_targets = resolve_peer_tcp_addrs(*udp_src, &peer.addresses);
                    for tcp_addr in tcp_targets {
                        let _ = tx.send(WorkerMsg::Progress {
                            pct: 25,
                            status: format!("Connecting TLS 1.3 to {}...", tcp_addr),
                            file: "-".to_owned(),
                        });

                        let tx_ev = tx.clone();
                        let sync_res = connect_and_sync(
                            &identity,
                            tcp_addr,
                            Some(peer.id),
                            &our_hello,
                            &storage,
                            Duration::from_secs(4),
                            move |event| {
                                let _ = tx_ev.send(WorkerMsg::SyncEvent(event));
                            },
                        );

                        match sync_res {
                            Ok(hs) => {
                                let is_active =
                                    matches!(hs.session_status, SyncSessionStatus::Active { .. });
                                let summary = match hs.session_status {
                                    SyncSessionStatus::AwaitingPairing => {
                                        "Awaiting device pairing in Syncthing Web UI.".to_owned()
                                    }
                                    SyncSessionStatus::ClosedByPeer(reason) => {
                                        format!("Session closed by peer: {}", reason)
                                    }
                                    SyncSessionStatus::Active {
                                        files_up_to_date,
                                        files_downloaded,
                                        bytes_downloaded,
                                        blocks_uploaded,
                                        bytes_uploaded,
                                        ..
                                    } => {
                                        format!(
                                            "Sync OK: {} up-to-date | {} DL ({} KB) | {} UL ({} B)",
                                            files_up_to_date,
                                            files_downloaded,
                                            bytes_downloaded / 1024,
                                            blocks_uploaded,
                                            bytes_uploaded
                                        )
                                    }
                                };
                                if is_active {
                                    let _ = tx.send(WorkerMsg::Finished(summary));
                                    return;
                                }
                                last_summary = Some(summary);
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerMsg::Log(format!(
                                    "Error connecting to {}: {}",
                                    tcp_addr, e
                                )));
                            }
                        }
                    }
                }
                let final_msg =
                    last_summary.unwrap_or_else(|| "Connection attempts finished.".to_owned());
                let _ = tx.send(WorkerMsg::Finished(final_msg));
            }
            Err(e) => {
                let _ = tx.send(WorkerMsg::Finished(format!("UDP discovery error: {}", e)));
            }
        }
    });

    rx
}
