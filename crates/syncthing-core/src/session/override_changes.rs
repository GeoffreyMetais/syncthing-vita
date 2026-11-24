//! Override of a send-only folder's remote changes, requested by the application through
//! [`SyncControl::request_override`](super::control::SyncControl::request_override) and run in
//! the persistent loop, where the peer's index is known.

use std::io;

use super::context::SessionContext;
use super::control::SyncControl;
use super::events::SyncEvent;
use super::folder_requests::{handle_requests, RequestStep, RequestsOutcome};
use super::folders::FolderSyncState;
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
    let outcome = handle_requests(
        stream,
        ctx,
        folders,
        &requested,
        |stream, ctx, folder_id, remote| {
            let own_id = ctx.own_id();
            match storage.override_remote_changes(folder_id, own_id, remote, &mut || {
                send_keepalive_if_due(stream, ctx)
            }) {
                Ok(overridden) => Ok(RequestStep::Done(overridden)),
                Err(e) => {
                    ctx.emit(SyncEvent::FolderUnavailable {
                        folder: folder_id.to_owned(),
                        error: e.to_string(),
                    });
                    Ok(RequestStep::Skipped)
                }
            }
        },
        |ctx, folder_id, overridden| {
            ctx.emit(SyncEvent::RemoteChangesOverridden {
                folder: folder_id.to_owned(),
                overridden,
            });
        },
    )?;
    Ok(!matches!(outcome, RequestsOutcome::Idle))
}
