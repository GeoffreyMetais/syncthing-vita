//! Initial exchange right after `Hello`: the peer's `ClusterConfig` (answered with ours),
//! followed by its initial `Index` messages.

use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::context::SessionContext;
use super::events::SyncEvent;
use super::folders::FolderSyncState;
use super::upload::serve_block_request;
use crate::bep::{
    write_bep_message, BepClose, BepClusterConfig, BepFolder, BepHello, BepIndex, BepMessageType,
    FolderIndexState,
};
use crate::tls::polling::try_read_bep_message;
use crate::tls::BepTlsStream;

/// Total time allowed for the initial exchange.
const EXCHANGE_WINDOW: Duration = Duration::from_secs(4);
/// Poll timeout while waiting for the peer's `ClusterConfig`.
const AWAIT_CONFIG_POLL: Duration = Duration::from_millis(1600);
/// Poll timeout while collecting initial `Index` messages.
const AWAIT_INDEX_POLL: Duration = Duration::from_millis(650);

/// Result of the initial exchange.
pub enum ClusterExchange {
    /// `ClusterConfig`s were exchanged; `folders` holds shared folders and remote files.
    Established,
    /// The peer did not send a `ClusterConfig` (it has not accepted / paired this device yet).
    AwaitingPairing,
    /// The peer sent `Close` with this reason.
    ClosedByPeer(String),
}

/// Runs the initial exchange, filling `folders` with the shared folders and the remote files.
pub fn exchange_cluster_config(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    peer_addr: SocketAddr,
    peer_hello: &BepHello,
) -> io::Result<ClusterExchange> {
    let mut got_cluster_config = false;
    let start = Instant::now();

    while start.elapsed() < EXCHANGE_WINDOW {
        let poll_timeout = if got_cluster_config {
            AWAIT_INDEX_POLL
        } else {
            AWAIT_CONFIG_POLL
        };
        match try_read_bep_message(stream, poll_timeout) {
            Ok(None) => break,
            Ok(Some((BepMessageType::ClusterConfig, payload))) => {
                let cfg = BepClusterConfig::decode(&payload)?;
                if cfg.secondary {
                    continue;
                }
                if !got_cluster_config {
                    got_cluster_config = true;
                    let peer_device_id = ctx.peer_device_id;
                    ctx.emit(SyncEvent::PeerConnected {
                        peer_addr,
                        peer_device_id,
                        peer_name: peer_hello.device_name.clone(),
                        client_version: peer_hello.client_version.clone(),
                    });
                }
                accept_peer_cluster_config(ctx, folders, cfg);
                let reply = BepClusterConfig::encode_for_peers(
                    &initial_folder_states(ctx, &folders.shared_folders),
                    ctx.own_id(),
                    ctx.peer_device_id,
                );
                write_bep_message(stream, BepMessageType::ClusterConfig, &reply)?;
            }
            Ok(Some((BepMessageType::Index | BepMessageType::IndexUpdate, payload))) => {
                let idx = BepIndex::decode(&payload)?;
                if folders.is_shared_folder(&idx.folder) {
                    folders.merge_remote_index(&idx.folder, &idx.files);
                }
            }
            Ok(Some((BepMessageType::Request, payload))) => {
                serve_block_request(stream, ctx, &payload)?;
            }
            Ok(Some((BepMessageType::Ping, _))) => {
                write_bep_message(stream, BepMessageType::Ping, &[])?;
            }
            Ok(Some((BepMessageType::Close, payload))) => {
                let close = BepClose::decode(&payload)?;
                return Ok(ClusterExchange::ClosedByPeer(close.reason));
            }
            Ok(Some(_)) => {}
            Err(_) => break,
        }
    }

    Ok(if got_cluster_config {
        ClusterExchange::Established
    } else {
        ClusterExchange::AwaitingPairing
    })
}

/// Records the peer's folders that the local configuration shares with it, then adds local
/// shares explicitly shared with this peer. Every announced folder is still reported through
/// `ClusterConfigReceived` so the application can ask the user for consent.
fn accept_peer_cluster_config(
    ctx: &mut SessionContext<'_>,
    folders: &mut FolderSyncState,
    cfg: BepClusterConfig,
) {
    let peer_device_id = ctx.peer_device_id;
    let accepted: Vec<BepFolder> = cfg
        .folders
        .iter()
        .filter(|f| ctx.storage.is_folder_shared_with(&f.id, &peer_device_id))
        .cloned()
        .collect();
    folders.peer_folder_ids = accepted.iter().map(|f| f.id.clone()).collect();
    folders.shared_folders = accepted;
    ctx.emit(SyncEvent::ClusterConfigReceived {
        peer_device_id,
        folders: cfg.folders,
    });

    for lf in ctx.storage.local_folders() {
        let shared_with_peer = lf
            .device_ids
            .iter()
            .any(|id| id.matches_peer(&peer_device_id));
        if shared_with_peer && !folders.shared_folders.iter().any(|f| f.id == lf.id) {
            folders.shared_folders.push(lf.clone());
        }
    }
}

/// Local index state of each shared folder, so our single `ClusterConfig` advertises accurate
/// `index_id` and `max_sequence` values.
fn initial_folder_states(
    ctx: &SessionContext<'_>,
    shared_folders: &[BepFolder],
) -> Vec<(BepFolder, FolderIndexState)> {
    shared_folders
        .iter()
        .map(|f| {
            let state = if ctx.storage.is_folder_paused(&f.id) {
                FolderIndexState {
                    index_id: 0,
                    max_sequence: 0,
                    paused: true,
                }
            } else {
                ctx.storage
                    .build_local_index(&f.id, ctx.own_id(), &[])
                    .map(|(_, st)| st)
                    .unwrap_or_default()
            };
            (f.clone(), state)
        })
        .collect()
}
