//! Coordinates messages (`WorkerMsg` & `SyncEvent`) from the background BEP thread into `AppState` and `ShareService`.

use std::sync::mpsc::Receiver;

use syncthing_core::SyncEvent;

use crate::application::ShareService;
use crate::domain::AppState;
use crate::infrastructure::{ConfigRepository, WorkerMsg};

pub struct SyncCoordinator;

impl SyncCoordinator {
    /// Drains all pending messages from `worker_rx`, updating `AppState` and persisting discovered
    /// devices/folders via `ShareService`.
    pub fn process_worker_messages(
        state: &mut AppState,
        repo: &ConfigRepository,
        worker_rx: &mut Option<Receiver<WorkerMsg>>,
    ) {
        let mut disconnected = false;
        if let Some(rx) = worker_rx.as_ref() {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMsg::Log(line) => ShareService::log_event(state, repo, &line),
                    WorkerMsg::Progress { pct, status, file } => {
                        state.progress_pct = pct;
                        state.status_line = status;
                        state.current_file = file;
                    }
                    WorkerMsg::DeviceDiscovered {
                        device_id,
                        name,
                        address,
                    } => {
                        ShareService::register_discovered_peer(
                            state, repo, device_id, &name, &address,
                        );
                    }
                    WorkerMsg::SyncEvent(event) => Self::handle_sync_event(state, repo, event),
                    WorkerMsg::Finished {
                        summary,
                        allow_auto_reconnect,
                    } => {
                        state.sync_in_progress = false;
                        state.session_connected = false;
                        state.auto_reconnect_enabled = allow_auto_reconnect;
                        state.progress_pct = 100;
                        state.current_file = "Finished".to_owned();
                        state.status_line = summary.clone();
                        ShareService::log_event(state, repo, &summary);
                        disconnected = true;
                    }
                }
            }
        }
        if disconnected {
            *worker_rx = None;
        }
    }

    fn handle_sync_event(state: &mut AppState, repo: &ConfigRepository, event: SyncEvent) {
        match event {
            SyncEvent::PeerConnected {
                peer_addr,
                peer_device_id,
                peer_name,
                client_version,
            } => {
                let name = if peer_name.is_empty() {
                    format!("Device {}", peer_device_id.short())
                } else {
                    peer_name
                };
                ShareService::upsert_device(
                    state,
                    repo,
                    peer_device_id,
                    &name,
                    &peer_addr.to_string(),
                );
                state.peer_label = format!("{} @ {} ({})", name, peer_addr, client_version);
                state.status_line = "Exchanging ClusterConfig & BEP Index...".to_owned();
                state.progress_pct = 40;
                ShareService::log_event(
                    state,
                    repo,
                    &format!("Connected via TLS 1.3 to {} ({})", name, peer_addr),
                );
            }
            SyncEvent::ClusterConfigReceived {
                peer_device_id,
                folders,
            } => {
                let mut labels = Vec::new();
                for f in &folders {
                    ShareService::upsert_discovered_folder(
                        state,
                        repo,
                        peer_device_id,
                        &f.id,
                        &f.label,
                    );
                    labels.push(format!("{} ({})", f.id, f.label));
                }
                ShareService::refresh_picker_items(state, repo);
                let joined = if labels.is_empty() {
                    "No remote folders shared".to_owned()
                } else {
                    labels.join(", ")
                };
                state.folder_label = joined.clone();
                state.progress_pct = 55;
                ShareService::log_event(
                    state,
                    repo,
                    &format!("Received shares from peer: {}", joined),
                );
            }
            SyncEvent::FileUpToDate { name, size, .. } => {
                state.files_up_to_date += 1;
                state.current_file = name.clone();
                state.status_line = format!("Up-to-date (Delta 0 B): {}", name);
                ShareService::log_event(
                    state,
                    repo,
                    &format!("Already up-to-date (SHA-256 OK): {} ({} B)", name, size),
                );
            }
            SyncEvent::BlockDownloaded {
                name,
                offset,
                size,
                total_size,
                ..
            } => {
                state.sync_in_progress = true;
                state.bytes_transferred += size;
                state.current_file = name.clone();
                let pct = if total_size > 0 {
                    (((offset + size as u64) * 100) / total_size) as u32
                } else {
                    100
                };
                state.progress_pct = pct.clamp(55, 95);
                state.status_line = format!("Downloading {} ({}%)", name, pct);
            }
            SyncEvent::FileSynced {
                name,
                blocks_downloaded,
                bytes_downloaded,
                ..
            } => {
                state.files_downloaded += 1;
                state.current_file = name.clone();
                ShareService::log_event(
                    state,
                    repo,
                    &format!(
                        "Synced: {} ({} block(s), {} bytes)",
                        name, blocks_downloaded, bytes_downloaded
                    ),
                );
            }
            SyncEvent::LocalIndexPublished { folder, file_count } => {
                state.progress_pct = 90;
                state.status_line = format!("Local Vita index published ({} files)", file_count);
                ShareService::log_event(
                    state,
                    repo,
                    &format!("Local index sent for {} ({} files)", folder, file_count),
                );
            }
            SyncEvent::BlockUploaded {
                name, offset, size, ..
            } => {
                state.blocks_uploaded += 1;
                state.bytes_transferred += size;
                state.current_file = name.clone();
                state.status_line = format!("Uploading to peer: {}", name);
                ShareService::log_event(
                    state,
                    repo,
                    &format!(
                        "Block uploaded (Vita -> PC/Mac): {} (offset={}, {} B)",
                        name, offset, size
                    ),
                );
            }
            SyncEvent::SyncCycleCompleted {
                active_connections,
                files_up_to_date,
                files_downloaded,
                bytes_downloaded,
                blocks_uploaded,
                bytes_uploaded,
            } => {
                state.sync_in_progress = false;
                state.session_connected = true;
                state.auto_reconnect_enabled = true;
                state.progress_pct = 100;
                let conn_badge = if active_connections > 1 {
                    format!("1+{} connections", active_connections - 1)
                } else {
                    "1 connection".to_owned()
                };
                state.current_file = format!("Connected ({})", conn_badge);
                let summary = format!(
                    "Connected ({}) · {} up-to-date | {} DL ({} KB) | {} UL ({} B)",
                    conn_badge,
                    files_up_to_date,
                    files_downloaded,
                    bytes_downloaded / 1024,
                    blocks_uploaded,
                    bytes_uploaded
                );
                state.status_line = summary.clone();
                ShareService::log_event(state, repo, &summary);
            }
            SyncEvent::FolderUnavailable { folder, error } => {
                ShareService::log_event(
                    state,
                    repo,
                    &format!("Folder {} skipped (local directory unavailable): {}", folder, error),
                );
            }
            SyncEvent::LocalChangeApplied {
                folder,
                description,
            } => {
                ShareService::log_event(state, repo, &format!("[{}] {}", folder, description));
            }
            SyncEvent::FileError {
                folder,
                name,
                error,
            } => {
                ShareService::log_event(
                    state,
                    repo,
                    &format!("Skipped file {}/{}: {}", folder, name, error),
                );
            }
            SyncEvent::SessionEnded {
                peer_device_id,
                reason,
            } => {
                ShareService::log_event(
                    state,
                    repo,
                    &format!(
                        "Session with {} ended ({}). Reconnecting...",
                        peer_device_id.short(),
                        reason
                    ),
                );
            }
        }
    }
}
