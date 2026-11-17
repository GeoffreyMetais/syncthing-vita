//! First synchronization cycle after the `ClusterConfig` exchange: initial download,
//! local index publication and draining of the peer's first upload requests.

use std::io;
use std::time::{Duration, Instant};

use super::context::SessionContext;
use super::control::SyncControl;
use super::dispatch::{dispatch_primary_message, next_primary_message, with_context, Dispatched};
use super::download::download_folder;
use super::folders::FolderSyncState;
use super::index_exchange::{build_initial_local_indexes, publish_initial_indexes};
use super::keepalive::send_keepalive_if_due;
use super::secondary::{poll_all_secondaries, wait_for_promotion, SecondaryBepConn};
use super::tuning::CycleDrain;
use crate::tls::BepTlsStream;

/// Drain window when a persistent session follows (it keeps serving requests afterwards).
const DRAIN_BEFORE_PERSISTENT: Duration = Duration::from_millis(1000);
/// Drain window for a session without a control handle.
const DRAIN_SINGLE_CYCLE: Duration = Duration::from_secs(4);
const DRAIN_POLL: Duration = Duration::from_millis(120);

/// How the initial cycle ended.
pub enum InitialSync {
    Completed,
    ClosedByPeer(String),
    /// A local stop was requested ([`SyncControl::request_stop`]) before the cycle was over.
    Stopped,
}

/// Drain of the initial cycle: fixed windows without a control handle or before a persistent
/// loop, the application's [`CycleDrain`] for a single-cycle handle.
pub fn drain_for(control: Option<&SyncControl>) -> CycleDrain {
    match control {
        None => CycleDrain::fixed(DRAIN_SINGLE_CYCLE),
        Some(ctrl) => ctrl
            .single_cycle_drain()
            .unwrap_or(CycleDrain::fixed(DRAIN_BEFORE_PERSISTENT)),
    }
}

/// Runs the initial cycle:
/// 1. downloads every non-paused folder from the peer's initial index (delta sync);
/// 2. builds and publishes our local `Index` (after secondary connections got promoted);
/// 3. drains upload requests and late `Index` messages (see [`CycleDrain`]) before reporting
///    the cycle complete.
///
/// With a single-cycle handle ([`SyncControl::single_cycle`]), a stop request is honoured
/// between the phases (and between blocks by the download) and ends the cycle with
/// [`InitialSync::Stopped`]. Other sessions keep their behaviour: only the download notices a
/// stop (as an error); a persistent session sees it in its loop, after reporting this cycle.
pub fn run_initial_sync(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    secondaries: &mut Vec<SecondaryBepConn>,
    secondaries_opened_at: Instant,
    drain: CycleDrain,
) -> io::Result<InitialSync> {
    let single_cycle = ctx
        .control
        .is_some_and(|c| c.single_cycle_drain().is_some());
    let stop_now = |ctx: &SessionContext<'_>| single_cycle && ctx.is_stop_requested();
    // In a single cycle, a download interrupted by a stop request is a stop, not a failure:
    // the session still ends with a `Close`.
    match initial_download(stream, ctx, folders) {
        Err(_) if stop_now(ctx) => return Ok(InitialSync::Stopped),
        result => result?,
    }
    if stop_now(ctx) {
        return Ok(InitialSync::Stopped);
    }

    build_initial_local_indexes(stream, ctx, folders);
    if stop_now(ctx) {
        return Ok(InitialSync::Stopped);
    }
    wait_for_promotion(secondaries, secondaries_opened_at, ctx);
    publish_initial_indexes(stream, ctx, folders)?;

    let drain_start = Instant::now();
    let mut last_activity = Instant::now();
    while !drain.is_over(drain_start.elapsed(), last_activity.elapsed()) {
        if stop_now(ctx) {
            return Ok(InitialSync::Stopped);
        }
        let uploaded_before = ctx.stats.blocks_uploaded;
        match next_primary_message(stream, ctx, DRAIN_POLL) {
            Ok(None) => {}
            Ok(Some((msg_type, payload))) => {
                last_activity = Instant::now();
                match dispatch_primary_message(stream, ctx, folders, msg_type, &payload, true)? {
                    Dispatched::Closed(reason) => return Ok(InitialSync::ClosedByPeer(reason)),
                    Dispatched::Handled | Dispatched::IndexApplied => {}
                }
            }
            Err(_) => break,
        }
        poll_all_secondaries(secondaries, ctx);
        // Blocks served on the secondary connections are activity too.
        if ctx.stats.blocks_uploaded != uploaded_before {
            last_activity = Instant::now();
        }
        // A single cycle's drain can last minutes; the fixed drains stay short.
        if single_cycle {
            send_keepalive_if_due(stream, ctx);
        }
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
    for (folder_id, files) in folders.remote_folders() {
        if ctx.storage.is_folder_paused(folder_id) {
            continue;
        }
        download_folder(stream, ctx, folder_id, files, true)
            .map_err(|e| with_context(&format!("Initial download ({folder_id})"), e))?;
    }
    Ok(())
}
