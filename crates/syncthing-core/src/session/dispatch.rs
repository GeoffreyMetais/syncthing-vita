//! Dispatch of messages received on the primary connection once the session is established.

use std::io;

use super::context::SessionContext;
use super::folders::FolderSyncState;
use super::index_exchange::{handle_runtime_cluster_config, handle_runtime_index};
use super::upload::serve_block_request;
use crate::bep::{write_bep_message, BepClose, BepMessageType};
use crate::tls::BepTlsStream;

/// What a dispatched primary-connection message resulted in.
pub enum Dispatched {
    /// Nothing the caller needs to react to.
    Handled,
    /// A remote index was applied to a non-paused folder (a sync cycle happened).
    IndexApplied,
    /// The peer sent `Close` with this reason.
    Closed(String),
}

/// Handles one message of an established session (`ClusterConfig`, `Index` / `IndexUpdate`,
/// `Request`, `Ping`, `Close`). Errors are prefixed with the failing stage.
pub fn dispatch_primary_message(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    msg_type: BepMessageType,
    payload: &[u8],
    count_up_to_date: bool,
) -> io::Result<Dispatched> {
    match msg_type {
        BepMessageType::ClusterConfig => {
            handle_runtime_cluster_config(stream, ctx, folders, payload);
            Ok(Dispatched::Handled)
        }
        BepMessageType::Index | BepMessageType::IndexUpdate => {
            match handle_runtime_index(stream, ctx, folders, payload, count_up_to_date) {
                Ok(true) => Ok(Dispatched::IndexApplied),
                Ok(false) => Ok(Dispatched::Handled),
                Err(e) => Err(with_context("index/download error", e)),
            }
        }
        BepMessageType::Request => serve_block_request(stream, ctx, payload)
            .map(|_| Dispatched::Handled)
            .map_err(|e| with_context("request handling error", e)),
        BepMessageType::Ping => write_bep_message(stream, BepMessageType::Ping, &[])
            .map(|_| Dispatched::Handled)
            .map_err(|e| with_context("ping reply failed", e)),
        BepMessageType::Close => {
            let reason = BepClose::decode(payload)
                .map(|c| c.reason)
                .unwrap_or_default();
            Ok(Dispatched::Closed(reason))
        }
        _ => Ok(Dispatched::Handled),
    }
}

/// Prefixes an I/O error with a human-readable stage/context while preserving its kind.
pub fn with_context(stage: &str, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{stage}: {e}"))
}
