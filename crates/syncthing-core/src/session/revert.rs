//! Revert of a receive-only folder's local changes, requested by the application through
//! [`SyncControl::request_revert`](super::control::SyncControl::request_revert) and run in the
//! persistent loop, where the peer's index is known.

use std::io;

use super::context::SessionContext;
use super::control::SyncControl;
use super::download::download_folder;
use super::events::SyncEvent;
use super::folders::FolderSyncState;
use super::index_exchange::publish_index;
use super::keepalive::send_keepalive_if_due;
use crate::tls::BepTlsStream;

/// Reverts every pending folder this session synchronizes (shared with the peer, receive-only,
/// not paused): local changes are dropped, the peer's version is pulled again and the result
/// is published. Returns `true` if a folder was reverted.
pub fn handle_revert_requests(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    control: &SyncControl,
) -> io::Result<bool> {
    let storage = ctx.storage;
    let requested = control.take_revert_requests(|folder_id| {
        folders.is_shared_folder(folder_id)
            && storage.is_folder_receive_only(folder_id)
            && !storage.is_folder_paused(folder_id)
    });
    for folder_id in &requested {
        let remote = folders.remote_files(folder_id).to_vec();
        let own_id = ctx.own_id();
        let reverted = match storage.revert_local_changes(folder_id, own_id, &remote, &mut || {
            send_keepalive_if_due(stream, ctx)
        }) {
            Ok(reverted) => reverted,
            Err(e) => {
                ctx.emit(SyncEvent::FolderUnavailable {
                    folder: folder_id.clone(),
                    error: e.to_string(),
                });
                continue;
            }
        };
        download_folder(stream, ctx, folder_id, &remote, false)?;
        let (index, _) = storage.local_index_snapshot(folder_id, own_id, &remote);
        publish_index(stream, ctx, folders, &index)?;
        folders.upsert_local_index(index);
        ctx.emit(SyncEvent::LocalChangesReverted {
            folder: folder_id.clone(),
            reverted,
        });
    }
    Ok(!requested.is_empty())
}
