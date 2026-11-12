//! Index exchange with the peer: building and publishing our local `Index` / `IndexUpdate`
//! messages and applying the peer's runtime `ClusterConfig` / `Index` changes.

use std::io;

use super::context::SessionContext;
use super::download::download_folder;
use super::events::SyncEvent;
use super::folders::{published_file_count, FolderSyncState};
use crate::bep::{write_bep_message, BepClusterConfig, BepIndex, BepMessageType};
use crate::tls::BepTlsStream;

/// Builds the local index of every non-paused shared folder after the initial download.
/// An unreadable local folder is reported and skipped, never fatal for the session.
pub fn build_initial_local_indexes(ctx: &mut SessionContext<'_>, folders: &mut FolderSyncState) {
    let folder_ids: Vec<String> = folders
        .shared_folders
        .iter()
        .map(|f| f.id.clone())
        .collect();
    for folder_id in folder_ids {
        if ctx.storage.is_folder_paused(&folder_id) {
            continue;
        }
        let built = ctx.storage.build_local_index(
            &folder_id,
            ctx.own_id(),
            folders.remote_files(&folder_id),
        );
        match built {
            Ok((local_idx, state)) => {
                folders
                    .last_published_seq
                    .insert(folder_id.clone(), state.max_sequence);
                folders.local_indexes.push(local_idx);
            }
            Err(e) => ctx.emit(SyncEvent::FolderUnavailable {
                folder: folder_id,
                error: e.to_string(),
            }),
        }
    }
}

/// Publishes a full `Index` for every local index of a folder the peer shares with us.
///
/// In BEP/1.0 (Syncthing's `lib/protocol/protocol.go`), sending a second `ClusterConfig` after
/// transitioning to `stateReady` is a fatal protocol error (`cluster config message in state 1`)
/// that makes the peer close all connections, so only `Index` messages are sent here.
pub fn publish_initial_indexes(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
) -> io::Result<()> {
    let to_publish: Vec<BepIndex> = folders
        .local_indexes
        .iter()
        .filter(|idx| folders.is_peer_folder(&idx.folder))
        .cloned()
        .collect();
    for local_idx in &to_publish {
        write_bep_message(stream, BepMessageType::Index, &local_idx.encode())?;
        folders.mark_index_sent(&local_idx.folder);
        emit_published(ctx, local_idx);
    }
    Ok(())
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
        if !folders.shared_folders.iter().any(|sf| sf.id == f.id) {
            folders.shared_folders.push(f.clone());
        }
        if folders.is_peer_folder(&f.id) {
            continue;
        }
        folders.peer_folder_ids.push(f.id.clone());
        if !ctx.storage.is_folder_paused(&f.id) && folders.local_index(&f.id).is_none() {
            if let Ok((new_idx, st)) = ctx.storage.build_local_index(&f.id, ctx.own_id(), &[]) {
                folders
                    .last_published_seq
                    .insert(f.id.clone(), st.max_sequence);
                folders.local_indexes.push(new_idx);
            }
        }
        if let Some(local_idx) = folders.local_index(&f.id).cloned() {
            let _ = write_bep_message(stream, BepMessageType::Index, &local_idx.encode());
            folders.mark_index_sent(&local_idx.folder);
            emit_published(ctx, &local_idx);
        }
    }
    let peer_device_id = ctx.peer_device_id;
    ctx.emit(SyncEvent::ClusterConfigReceived {
        peer_device_id,
        folders: cfg.folders,
    });
}

/// Applies a runtime `Index` / `IndexUpdate`: downloads newer files, then republishes our
/// local index of that folder. Returns `true` if the folder was synchronized (not paused).
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

    let built =
        ctx.storage
            .build_local_index(&idx.folder, ctx.own_id(), folders.remote_files(&idx.folder));
    if let Ok((new_idx, state)) = built {
        folders
            .last_published_seq
            .insert(idx.folder.clone(), state.max_sequence);
        let msg_type = folders.next_index_message_type(&idx.folder);
        write_bep_message(stream, msg_type, &new_idx.encode())?;
        emit_published(ctx, &new_idx);
        folders.upsert_local_index(new_idx);
    }
    Ok(true)
}

/// Rescans every non-paused shared folder and publishes an `IndexUpdate` for those whose
/// sequence moved (or all of them on a `manual` rescan).
/// Returns `true` if a publication round happened (the caller then reports a sync cycle).
pub fn rescan_and_publish(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    manual: bool,
) -> io::Result<bool> {
    let mut any_changed = false;
    let mut changed_indexes = Vec::new();

    let folder_ids: Vec<String> = folders
        .shared_folders
        .iter()
        .map(|f| f.id.clone())
        .collect();
    for folder_id in folder_ids {
        if ctx.storage.is_folder_paused(&folder_id) {
            continue;
        }
        let built = ctx.storage.build_local_index(
            &folder_id,
            ctx.own_id(),
            folders.remote_files(&folder_id),
        );
        let Ok((local_idx, state)) = built else {
            continue;
        };
        let prev_seq = folders
            .last_published_seq
            .get(&folder_id)
            .copied()
            .unwrap_or(0);
        if state.max_sequence != prev_seq || manual {
            any_changed |= state.max_sequence != prev_seq;
            folders
                .last_published_seq
                .insert(folder_id.clone(), state.max_sequence);
            changed_indexes.push(local_idx.clone());
        }
        folders.upsert_local_index(local_idx);
    }

    if !any_changed && !manual {
        return Ok(false);
    }
    for local_idx in &changed_indexes {
        if !folders.is_peer_folder(&local_idx.folder) {
            continue;
        }
        let msg_type = folders.next_index_message_type(&local_idx.folder);
        write_bep_message(stream, msg_type, &local_idx.encode())?;
        emit_published(ctx, local_idx);
    }
    Ok(true)
}

fn emit_published(ctx: &mut SessionContext<'_>, index: &BepIndex) {
    ctx.emit(SyncEvent::LocalIndexPublished {
        folder: index.folder.clone(),
        file_count: published_file_count(index),
    });
}
