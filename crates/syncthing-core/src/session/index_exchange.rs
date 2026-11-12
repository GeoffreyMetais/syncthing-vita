//! Index exchange with the peer: building and publishing our local `Index` / `IndexUpdate`
//! messages and applying the peer's runtime `ClusterConfig` / `Index` changes.

use std::io;

use super::context::SessionContext;
use super::download::download_folder;
use super::events::SyncEvent;
use super::folders::{published_file_count, FolderSyncState};
use super::keepalive::send_keepalive_if_due;
use crate::bep::{write_bep_message, BepClusterConfig, BepIndex, FolderIndexState};
use crate::tls::BepTlsStream;

/// Scans `folder_id` and returns its local index, sending keepalive `Ping`s while a long scan
/// (first hashing of a large folder) is running.
pub fn scan_local_index(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &FolderSyncState,
    folder_id: &str,
) -> io::Result<(BepIndex, FolderIndexState)> {
    let storage = ctx.storage;
    let own_id = ctx.own_id();
    storage.build_local_index_with_progress(
        folder_id,
        own_id,
        folders.remote_files(folder_id),
        &mut || send_keepalive_if_due(stream, ctx),
    )
}

/// Builds the local index of every non-paused shared folder after the initial download.
/// An unreadable local folder is reported and skipped, never fatal for the session.
pub fn build_initial_local_indexes(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
) {
    let folder_ids: Vec<String> = folders
        .shared_folders
        .iter()
        .map(|f| f.id.clone())
        .collect();
    for folder_id in folder_ids {
        if ctx.storage.is_folder_paused(&folder_id) {
            continue;
        }
        match scan_local_index(stream, ctx, folders, &folder_id) {
            Ok((local_idx, _)) => folders.upsert_local_index(local_idx),
            Err(e) => ctx.emit(SyncEvent::FolderUnavailable {
                folder: folder_id,
                error: e.to_string(),
            }),
        }
    }
}

/// Sends what the peer does not know yet of `index` (see
/// [`FolderSyncState::take_unpublished`]), if the peer shares that folder with us.
/// Returns `true` if at least one message was sent.
fn publish_index(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    index: &BepIndex,
) -> io::Result<bool> {
    if !folders.is_peer_folder(&index.folder) {
        return Ok(false);
    }
    let messages = folders.take_unpublished(index);
    for (msg_type, batch) in &messages {
        write_bep_message(stream, *msg_type, &batch.encode())?;
    }
    if messages.is_empty() {
        return Ok(false);
    }
    ctx.emit(SyncEvent::LocalIndexPublished {
        folder: index.folder.clone(),
        file_count: published_file_count(index),
    });
    Ok(true)
}

/// Publishes every local index of a folder the peer shares with us.
///
/// In BEP/1.0 (Syncthing's `lib/protocol/protocol.go`), sending a second `ClusterConfig` after
/// transitioning to `stateReady` is a fatal protocol error (`cluster config message in state 1`)
/// that makes the peer close all connections, so only `Index` messages are sent here.
pub fn publish_initial_indexes(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
) -> io::Result<()> {
    let local_indexes = std::mem::take(&mut folders.local_indexes);
    let result = local_indexes
        .iter()
        .try_for_each(|index| publish_index(stream, ctx, folders, index).map(|_| ()));
    folders.local_indexes = local_indexes;
    result
}

/// Applies a `ClusterConfig` received after the initial exchange: newly announced folders that
/// are shared with this peer are added and immediately receive our local `Index`.
pub fn handle_runtime_cluster_config(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    payload: &[u8],
) {
    let Ok(cfg) = BepClusterConfig::decode(payload) else {
        return;
    };
    if cfg.secondary {
        return;
    }
    for f in &cfg.folders {
        if !ctx
            .storage
            .is_folder_shared_with(&f.id, &ctx.peer_device_id)
        {
            continue;
        }
        if !folders.is_shared_folder(&f.id) {
            folders.shared_folders.push(f.clone());
        }
        if folders.is_peer_folder(&f.id) {
            continue;
        }
        folders.peer_folder_ids.push(f.id.clone());
        if !ctx.storage.is_folder_paused(&f.id) && folders.local_index(&f.id).is_none() {
            if let Ok((new_idx, _)) = scan_local_index(stream, ctx, folders, &f.id) {
                folders.upsert_local_index(new_idx);
            }
        }
        if let Some(local_idx) = folders.local_index(&f.id).cloned() {
            let _ = publish_index(stream, ctx, folders, &local_idx);
        }
    }
    let peer_device_id = ctx.peer_device_id;
    ctx.emit(SyncEvent::ClusterConfigReceived {
        peer_device_id,
        folders: cfg.folders,
    });
}

/// Applies a runtime `Index` / `IndexUpdate`: downloads newer files, then publishes the
/// resulting changes of our local index. Returns `true` if the folder was synchronized
/// (shared and not paused).
pub fn handle_runtime_index(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    payload: &[u8],
    count_up_to_date: bool,
) -> io::Result<bool> {
    let Ok(idx) = BepIndex::decode(payload) else {
        return Ok(false);
    };
    if !folders.is_shared_folder(&idx.folder) {
        return Ok(false);
    }
    folders.merge_remote_index(&idx.folder, &idx.files);
    if ctx.storage.is_folder_paused(&idx.folder) {
        return Ok(false);
    }
    download_folder(stream, ctx, &idx.folder, &idx.files, count_up_to_date)?;

    // The pull just scanned the folder: publish from the database without a second scan.
    let (new_idx, _) = ctx.storage.local_index_snapshot(
        &idx.folder,
        ctx.own_id(),
        folders.remote_files(&idx.folder),
    );
    publish_index(stream, ctx, folders, &new_idx)?;
    folders.upsert_local_index(new_idx);
    Ok(true)
}

/// Rescans every non-paused shared folder and publishes the entries that changed since the
/// last publication. Returns `true` if a publication round happened (something was sent, or
/// the rescan was `manual`); the caller then reports a sync cycle.
pub fn rescan_and_publish(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    manual: bool,
) -> io::Result<bool> {
    let mut any_published = false;
    let folder_ids: Vec<String> = folders
        .shared_folders
        .iter()
        .map(|f| f.id.clone())
        .collect();
    for folder_id in folder_ids {
        if ctx.storage.is_folder_paused(&folder_id) {
            continue;
        }
        let Ok((local_idx, _)) = scan_local_index(stream, ctx, folders, &folder_id) else {
            continue;
        };
        any_published |= publish_index(stream, ctx, folders, &local_idx)?;
        folders.upsert_local_index(local_idx);
    }
    Ok(any_published || manual)
}
