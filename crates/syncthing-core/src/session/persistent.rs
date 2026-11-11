//! Persistent BEP/1.0 session loop: keeps the `1+2` connections open, answers `Ping` /
//! `Request` on all streams, applies remote `IndexUpdate`s and rescans local folders.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::context::SessionContext;
use super::control::SyncControl;
use super::dispatch::{dispatch_primary_message, Dispatched};
use super::events::SyncEvent;
use super::folders::FolderSyncState;
use super::index_exchange::rescan_and_publish;
use super::secondary::{
    open_secondary_bep_stream, ping_all_secondaries, poll_all_secondaries, SecondaryBepConn,
};
use crate::bep::{write_bep_message, BepClose, BepHello, BepMessageType};
use crate::tls::polling::try_read_bep_message;
use crate::tls::BepTlsStream;

const PRIMARY_POLL: Duration = Duration::from_millis(150);
const SECONDARY_RETRY_INTERVAL: Duration = Duration::from_secs(15);
const SECONDARY_RETRY_TIMEOUT: Duration = Duration::from_millis(1500);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
const RESCAN_INTERVAL: Duration = Duration::from_secs(12);

/// Connections and peer information owned by the persistent loop.
pub struct PersistentLinks<'h> {
    pub peer_addr: SocketAddr,
    pub our_hello: &'h BepHello,
    pub secondaries: Vec<SecondaryBepConn>,
    pub desired_secondary: usize,
}

impl PersistentLinks<'_> {
    fn active_connections(&self) -> usize {
        1 + self.secondaries.len()
    }
}

/// Runs the persistent loop until `control` requests a stop or the primary connection ends,
/// then closes every connection cleanly and emits `SessionEnded`.
pub fn run_persistent_session(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    mut links: PersistentLinks<'_>,
    control: &SyncControl,
) {
    let reason = match session_loop(stream, ctx, folders, &mut links, control) {
        Ok(()) => String::from("stopped locally"),
        Err(reason) => reason,
    };

    let close_payload = BepClose {
        reason: "closing".to_owned(),
    }
    .encode();
    for mut sec in links.secondaries {
        let _ = write_bep_message(&mut sec.stream, BepMessageType::Close, &close_payload);
    }
    let _ = write_bep_message(stream, BepMessageType::Close, &close_payload);
    let peer_device_id = ctx.peer_device_id;
    ctx.emit(SyncEvent::SessionEnded {
        peer_device_id,
        reason,
    });
}

/// Returns `Ok(())` on a local stop request, or `Err(reason)` when the session ended otherwise.
fn session_loop(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    links: &mut PersistentLinks<'_>,
    control: &SyncControl,
) -> Result<(), String> {
    let mut last_ping = Instant::now();
    let mut last_local_scan = Instant::now();
    let mut last_secondary_retry = Instant::now();
    let mut seen_rescan_epoch = control.rescan_epoch();

    while !control.is_stop_requested() {
        // A. Primary connection
        if poll_primary(stream, ctx, folders, links, control)? {
            return Ok(());
        }

        // B. Secondary data connections (`+2`)
        poll_all_secondaries(&mut links.secondaries, ctx);

        // C. Re-open a secondary connection that dropped transiently
        if links.secondaries.len() < links.desired_secondary
            && last_secondary_retry.elapsed() >= SECONDARY_RETRY_INTERVAL
        {
            last_secondary_retry = Instant::now();
            if let Ok(sec) = open_secondary_bep_stream(
                ctx,
                links.peer_addr,
                links.our_hello,
                SECONDARY_RETRY_TIMEOUT,
            ) {
                links.secondaries.push(sec);
            }
        }

        // D. Periodic `Ping` keepalives on all connections
        if last_ping.elapsed() >= KEEPALIVE_INTERVAL {
            last_ping = Instant::now();
            write_bep_message(stream, BepMessageType::Ping, &[])
                .map_err(|e| format!("keepalive ping failed: {e}"))?;
            ping_all_secondaries(&mut links.secondaries);
        }

        // E. Periodic or manual (UI-requested) local folder rescan
        let cur_epoch = control.rescan_epoch();
        let manual_rescan = cur_epoch != seen_rescan_epoch || control.take_rescan_request();
        seen_rescan_epoch = cur_epoch;
        if manual_rescan || last_local_scan.elapsed() >= RESCAN_INTERVAL {
            last_local_scan = Instant::now();
            let published = rescan_and_publish(stream, ctx, folders, manual_rescan)
                .map_err(|e| format!("index update send failed: {e}"))?;
            if published {
                ctx.emit_cycle_completed(links.active_connections());
            }
        }
    }
    Ok(())
}

/// Drains the primary connection. Returns `Ok(true)` if a local stop was requested meanwhile.
fn poll_primary(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    links: &PersistentLinks<'_>,
    control: &SyncControl,
) -> Result<bool, String> {
    loop {
        if control.is_stop_requested() {
            return Ok(true);
        }
        let (msg_type, payload) = match try_read_bep_message(stream, PRIMARY_POLL) {
            Ok(None) => return Ok(false),
            Ok(Some(msg)) => msg,
            Err(e) => return Err(format!("primary connection lost: {e}")),
        };
        match dispatch_primary_message(stream, ctx, folders, msg_type, &payload, false) {
            Ok(Dispatched::Handled) => {}
            Ok(Dispatched::IndexApplied) => ctx.emit_cycle_completed(links.active_connections()),
            Ok(Dispatched::Closed(reason)) => return Err(format!("closed by peer: {reason}")),
            Err(e) => return Err(e.to_string()),
        }
    }
}
