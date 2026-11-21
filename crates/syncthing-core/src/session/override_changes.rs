//! Override of a send-only folder's remote changes, requested by the application through
//! [`SyncControl::request_override`](super::control::SyncControl::request_override) and run in
//! the persistent loop, where the peer's index is known.

use std::io;

use super::context::SessionContext;
use super::control::SyncControl;
use super::events::SyncEvent;
use super::folders::FolderSyncState;
use super::index_exchange::publish_index;
use super::keepalive::send_keepalive_if_due;
use crate::tls::BepTlsStream;

/// Overrides remote changes for every pending send-only folder this session synchronizes
/// (shared with the peer, send-only, not paused): bumps local versions above remote versions,
/// creates deletion tombstones for remote-only files, and publishes the updated `Index`.
/// Returns `true` if at least one folder was overridden.
pub fn handle_override_requests(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    control: &SyncControl,
) -> io::Result<bool> {
    let storage = ctx.storage;
    let requested = control.take_override_requests(|folder_id| {
        folders.is_shared_folder(folder_id)
            && storage.is_folder_send_only(folder_id)
            && !storage.is_folder_paused(folder_id)
    });
    for folder_id in &requested {
        let remote = folders.remote_files(folder_id).to_vec();
        let own_id = ctx.own_id();
        let overridden =
            match storage.override_remote_changes(folder_id, own_id, &remote, &mut || {
                send_keepalive_if_due(stream, ctx)
            }) {
                Ok(overridden) => overridden,
                Err(e) => {
                    ctx.emit(SyncEvent::FolderUnavailable {
                        folder: folder_id.clone(),
                        error: e.to_string(),
                    });
                    continue;
                }
            };
        let (index, _) = storage.local_index_snapshot(folder_id, own_id, &remote);
        publish_index(stream, ctx, folders, &index)?;
        folders.upsert_local_index(index);
        ctx.emit(SyncEvent::RemoteChangesOverridden {
            folder: folder_id.clone(),
            overridden,
        });
    }
    Ok(!requested.is_empty())
}
