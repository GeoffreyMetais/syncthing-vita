//! Shared driver for the UI-requested per-folder operations (`SyncControl::request_revert` on
//! receive-only folders, `request_override` on send-only folders): for every requested folder
//! this session synchronizes, run the operation against the peer's index (with keepalives
//! during long local work), then publish the updated local `Index` and emit the result event.

use std::io;

use super::context::SessionContext;
use super::folders::FolderSyncState;
use super::index_exchange::publish_index;
use crate::bep::BepFileInfo;
use crate::tls::BepTlsStream;

/// What one folder's step produced.
pub(crate) enum RequestStep {
    /// The operation failed; `FolderUnavailable` was already emitted: skip to the next folder.
    Skipped,
    /// The operation applied `count` changes.
    Done(usize),
    /// The peer sent a BEP `Close` frame while handling this folder: stop the whole loop.
    Closed(String),
}

/// Outcome of handling a batch of requested folders.
pub(crate) enum RequestsOutcome {
    /// No folder was requested.
    Idle,
    /// At least one folder was requested.
    Done,
    /// The peer sent a BEP `Close` frame with this reason while handling a folder.
    ClosedByPeer(String),
}

/// Runs `step` for every folder in `requested`, publishing the updated local `Index` and
/// emitting `event` for each one that completed. `step` receives the peer's files of the
/// folder and performs the storage operation (and any follow-up, e.g. re-downloading after a
/// revert), sending keepalives itself while working.
pub(crate) fn handle_requests(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    requested: &[String],
    mut step: impl FnMut(
        &mut BepTlsStream,
        &mut SessionContext<'_>,
        &str,
        &[BepFileInfo],
    ) -> io::Result<RequestStep>,
    emit: impl Fn(&mut SessionContext<'_>, &str, usize),
) -> io::Result<RequestsOutcome> {
    for folder_id in requested {
        let remote = folders.remote_files(folder_id).to_vec();
        match step(stream, ctx, folder_id, &remote)? {
            RequestStep::Skipped => continue,
            RequestStep::Closed(reason) => return Ok(RequestsOutcome::ClosedByPeer(reason)),
            RequestStep::Done(count) => {
                let own_id = ctx.own_id();
                let (index, _) = ctx.storage.local_index_snapshot(folder_id, own_id, &remote);
                publish_index(stream, ctx, folders, &index)?;
                folders.upsert_local_index(index);
                emit(ctx, folder_id, count);
            }
        }
    }
    Ok(if requested.is_empty() {
        RequestsOutcome::Idle
    } else {
        RequestsOutcome::Done
    })
}
