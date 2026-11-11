//! First synchronization cycle after the `ClusterConfig` exchange: initial download,
//! local index publication and draining of the peer's first upload requests.

use std::io;
use std::time::{Duration, Instant};

use super::context::SessionContext;
use super::dispatch::{dispatch_primary_message, with_context, Dispatched};
use super::download::download_folder;
use super::folders::FolderSyncState;
use super::index_exchange::{build_initial_local_indexes, publish_initial_indexes};
use super::secondary::{poll_all_secondaries, wait_for_promotion, SecondaryBepConn};
use crate::tls::polling::try_read_bep_message;
use crate::tls::BepTlsStream;

/// Drain window when a persistent session follows (it keeps serving requests afterwards).
const DRAIN_BEFORE_PERSISTENT: Duration = Duration::from_millis(1000);
/// Drain window for a single-cycle session.
const DRAIN_SINGLE_CYCLE: Duration = Duration::from_secs(4);
const DRAIN_POLL: Duration = Duration::from_millis(120);

/// How the initial cycle ended.
pub enum InitialSync {
    Completed,
    ClosedByPeer(String),
}

/// Runs the initial cycle:
/// 1. downloads every non-paused folder from the peer's initial index (delta sync);
/// 2. builds and publishes our local `Index` (after secondary connections got promoted);
/// 3. drains upload requests and late `Index` messages before reporting the cycle complete.
pub fn run_initial_sync(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    secondaries: &mut Vec<SecondaryBepConn>,
    secondaries_opened_at: Instant,
    persistent: bool,
) -> io::Result<InitialSync> {
    initial_download(stream, ctx, folders)?;

    build_initial_local_indexes(ctx, folders);
    wait_for_promotion(secondaries, secondaries_opened_at, ctx);
    publish_initial_indexes(stream, ctx, folders)?;

    let drain_window = if persistent {
        DRAIN_BEFORE_PERSISTENT
    } else {
        DRAIN_SINGLE_CYCLE
    };
    let drain_start = Instant::now();
    while drain_start.elapsed() < drain_window {
        match try_read_bep_message(stream, DRAIN_POLL) {
            Ok(None) => {}
            Ok(Some((msg_type, payload))) => {
                match dispatch_primary_message(stream, ctx, folders, msg_type, &payload, true)? {
                    Dispatched::Closed(reason) => return Ok(InitialSync::ClosedByPeer(reason)),
                    Dispatched::Handled | Dispatched::IndexApplied => {}
                }
            }
            Err(_) => break,
        }
        poll_all_secondaries(secondaries, ctx);
    }

    ctx.emit_cycle_completed(1 + secondaries.len());
    Ok(InitialSync::Completed)
}

/// Delta-sync download of every non-paused folder announced in the peer's initial index.
fn initial_download(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &FolderSyncState,
) -> io::Result<()> {
    for (folder_id, files) in &folders.remote_files_by_folder {
        if ctx.storage.is_folder_paused(folder_id) {
            continue;
        }
        download_folder(stream, ctx, folder_id, files, true)
            .map_err(|e| with_context(&format!("Initial download ({folder_id})"), e))?;
    }
    Ok(())
}
