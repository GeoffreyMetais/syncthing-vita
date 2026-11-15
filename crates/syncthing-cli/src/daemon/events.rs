//! Turns the session events into log lines and sync activity.

use syncthing_core::{BepFolder, SyncEvent};

use super::context::{DaemonContext, KnownPeer};
use super::log::log;

/// Formats a byte count (`1.5 MiB`).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Handles one event of the session with `peer`.
pub fn handle_event(ctx: &DaemonContext, peer: &KnownPeer, event: SyncEvent) {
    match event {
        SyncEvent::PeerConnected {
            peer_addr,
            peer_name,
            client_version,
            ..
        } => log(&format!(
            "Connected to {} ({peer_name}, {client_version}) at {peer_addr}",
            peer.name
        )),
        SyncEvent::ClusterConfigReceived { folders, .. } => {
            report_unshared_folders(ctx, peer, &folders)
        }
        SyncEvent::FileSynced {
            folder,
            name,
            total_size,
            ..
        } => {
            ctx.table().activity.record_transfer();
            log(&format!(
                "{folder}: received {name} ({})",
                human_bytes(total_size)
            ));
        }
        SyncEvent::BlockDownloaded { .. } | SyncEvent::BlockUploaded { .. } => {
            ctx.table().activity.record_transfer();
        }
        SyncEvent::LocalChangeApplied {
            folder,
            description,
        } => {
            ctx.table().activity.record_transfer();
            log(&format!("{folder}: {description}"));
        }
        SyncEvent::LocalChangesReverted { folder, reverted } => {
            log(&format!("{folder}: {reverted} local change(s) reverted"));
        }
        SyncEvent::FolderUnavailable { folder, error } => {
            log(&format!("{folder}: folder unavailable: {error}"));
        }
        SyncEvent::FileError {
            folder,
            name,
            error,
        } => log(&format!("{folder}: {name}: {error}")),
        SyncEvent::SyncCycleCompleted {
            peer_device_id,
            files_downloaded,
            bytes_downloaded,
            bytes_uploaded,
            ..
        } => {
            let first = ctx.first_report(&format!("in-sync:{peer_device_id}"), "");
            ctx.table().activity.record_cycle(peer_device_id);
            if first || files_downloaded > 0 || bytes_uploaded > 0 {
                log(&format!(
                    "In sync with {} (session: {files_downloaded} file(s), {} received, {} sent)",
                    peer.name,
                    human_bytes(bytes_downloaded),
                    human_bytes(bytes_uploaded)
                ));
            }
        }
        SyncEvent::SessionEnded { reason, .. } => {
            log(&format!("Disconnected from {}: {reason}", peer.name));
        }
        SyncEvent::FileUpToDate { .. } | SyncEvent::LocalIndexPublished { .. } => {}
    }
}

/// A folder the peer shares with us that is not shared with it here: tell the user once how
/// to accept it (nothing is synchronized without consent).
fn report_unshared_folders(ctx: &DaemonContext, peer: &KnownPeer, folders: &[BepFolder]) {
    for folder in folders {
        if ctx.storage.is_folder_shared_with(&folder.id, &peer.id) {
            continue;
        }
        if ctx.first_report(&format!("offer:{}", peer.id), &folder.id) {
            log(&format!(
                "{} offers folder \"{}\" ({}). To accept: syncthing-cli folder add {} <path> --device {}",
                peer.name,
                folder.label,
                folder.id,
                shell_word(&folder.id),
                peer.id.to_syncthing_string()
            ));
        }
    }
}

/// Quotes `s` for a POSIX shell when needed (folder IDs come from peers).
fn shell_word(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
