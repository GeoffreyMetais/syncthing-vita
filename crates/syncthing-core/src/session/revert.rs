//! Revert of a receive-only folder's local changes, requested by the application through
//! [`SyncControl::request_revert`](super::control::SyncControl::request_revert) and run in the
//! persistent loop, where the peer's index is known.

use std::io;

use super::context::SessionContext;
use super::control::SyncControl;
use super::download::{download_folder, DownloadOutcome};
use super::events::SyncEvent;
use super::folder_requests::{handle_requests, RequestStep, RequestsOutcome};
use super::folders::FolderSyncState;
use super::keepalive::send_keepalive_if_due;
use crate::tls::BepTlsStream;

/// Outcome of handling UI-requested reverts for receive-only folders.
pub enum RevertOutcome {
    /// No revert was requested for any synchronized receive-only folder.
    Idle,
    /// At least one folder was reverted and its updated index was published.
    Reverted,
    /// The peer sent a BEP `Close` frame with this reason while reverting.
    ClosedByPeer(String),
}

/// Reverts every pending folder this session synchronizes (shared with the peer, receive-only,
/// not paused): local changes are dropped, the peer's version is pulled again and the result
/// is published.
pub fn handle_revert_requests(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    control: &SyncControl,
) -> io::Result<RevertOutcome> {
    let storage = ctx.storage;
    let requested = control.take_revert_requests(|folder_id| {
        folders.is_shared_folder(folder_id)
            && storage.is_folder_receive_only(folder_id)
            && !storage.is_folder_paused(folder_id)
    });
    let outcome = handle_requests(
        stream,
        ctx,
        folders,
        &requested,
        |stream, ctx, folder_id, remote| {
            let own_id = ctx.own_id();
            let reverted = match storage.revert_local_changes(folder_id, own_id, remote, &mut || {
                send_keepalive_if_due(stream, ctx)
            }) {
                Ok(reverted) => reverted,
                Err(e) => {
                    ctx.emit(SyncEvent::FolderUnavailable {
                        folder: folder_id.to_owned(),
                        error: e.to_string(),
                    });
                    return Ok(RequestStep::Skipped);
                }
            };
            if let DownloadOutcome::ClosedByPeer(reason) =
                download_folder(stream, ctx, folder_id, remote, false)?
            {
                return Ok(RequestStep::Closed(reason));
            }
            Ok(RequestStep::Done(reverted))
        },
        |ctx, folder_id, reverted| {
            ctx.emit(SyncEvent::LocalChangesReverted {
                folder: folder_id.to_owned(),
                reverted,
            });
        },
    )?;
    Ok(match outcome {
        RequestsOutcome::Idle => RevertOutcome::Idle,
        RequestsOutcome::Done => RevertOutcome::Reverted,
        RequestsOutcome::ClosedByPeer(reason) => RevertOutcome::ClosedByPeer(reason),
    })
}
