//! Initial exchange right after `Hello`: the peer's `ClusterConfig` (answered with ours),
//! followed by its initial `Index` messages.

use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::context::SessionContext;
use super::events::SyncEvent;
use super::folders::FolderSyncState;
use super::index_exchange::scan_local_index;
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
    // The dialer waits for our `ClusterConfig` when it is a peer like us (it answers the one it
    // receives): when the peer connected to us, we announce our folders first.
    let mut announced: Option<Vec<String>> = None;
    if stream.is_server() {
        add_local_folders_shared_with_peer(ctx, folders);
        announced = Some(send_cluster_config(stream, ctx, folders)?);
    }
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
                let own_id = ctx.own_id();
                ctx.peer_compression = BepClusterConfig::decode_peer_compression(&payload, &own_id);
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
                if announced.as_deref() != Some(&shared_folder_ids(folders)) {
                    announced = Some(send_cluster_config(stream, ctx, folders)?);
                }
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

    add_local_folders_shared_with_peer(ctx, folders);
}

/// Adds the local shares explicitly shared with the peer to the session's shared folders.
fn add_local_folders_shared_with_peer(ctx: &SessionContext<'_>, folders: &mut FolderSyncState) {
    let peer_device_id = ctx.peer_device_id;
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

/// The session's shared folder IDs, sorted (the announced set, whatever its order).
fn shared_folder_ids(folders: &FolderSyncState) -> Vec<String> {
    let mut ids: Vec<String> = folders
        .shared_folders
        .iter()
        .map(|f| f.id.clone())
        .collect();
    ids.sort();
    ids
}

/// Sends our `ClusterConfig` for the session's shared folders; returns the folder IDs announced.
fn send_cluster_config(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &FolderSyncState,
) -> io::Result<Vec<String>> {
    let states = initial_folder_states(stream, ctx, folders);
    let config = BepClusterConfig::encode_for_peers(
        &states,
        ctx.own_id(),
        &ctx.hello.device_name,
        ctx.peer_device_id,
    );
    write_bep_message(stream, BepMessageType::ClusterConfig, &config)?;
    Ok(shared_folder_ids(folders))
}

/// Local index state of each shared folder, so our single `ClusterConfig` advertises accurate
/// `index_id` and `max_sequence` values.
fn initial_folder_states(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folders: &FolderSyncState,
) -> Vec<(BepFolder, FolderIndexState)> {
    folders
        .shared_folders
        .iter()
        .map(|f| {
            let folder_type = ctx.storage.folder_type(&f.id);
            let state = if ctx.storage.is_folder_paused(&f.id) {
                FolderIndexState {
                    index_id: 0,
                    max_sequence: 0,
                    paused: true,
                    folder_type,
                }
            } else {
                scan_local_index(stream, ctx, folders, &f.id)
                    .map(|(_, st)| st)
                    .unwrap_or_default()
            };
            (
                f.clone(),
                FolderIndexState {
                    folder_type,
                    ..state
                },
            )
        })
        .collect()
}
