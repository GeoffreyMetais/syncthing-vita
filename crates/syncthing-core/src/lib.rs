//! `syncthing-core`: OS-agnostic Syncthing protocol implementation (BEP/1.0, TLS 1.3 & Local Discovery v4).
//! Tested natively on `x86_64-unknown-linux-gnu` via `cargo test -p syncthing-core`
//! (tests live in `crates/syncthing-core/tests/`).
//!
//! Layering (dependencies point downwards only):
//!
//! | Layer | Modules | I/O |
//! | :--- | :--- | :--- |
//! | Wire codecs & identity | [`proto`], [`device_id`], [`bep`], [`discovery::announce`], [`cert`], [`version_vector`] | none |
//! | Local persistence | [`storage`], [`local_db`], [`folder_sync`] | filesystem |
//! | Transport | [`tls`], [`discovery`] (UDP / LAN scan) | sockets |
//! | Sync engine | [`session`] | orchestrates all of the above |
//!
//! This file only declares modules and re-exports the public API.

pub mod bep;
pub mod cert;
pub mod device_id;
pub mod discovery;
pub mod folder_sync;
pub mod local_db;
pub mod proto;
pub mod relay;
pub mod session;
pub mod storage;
pub mod tls;
pub mod version_vector;

pub use bep::{
    compress_bep_lz4, decompress_bep_lz4, read_bep_message, write_bep_message,
    write_bep_message_compressed, BepBlockInfo, BepClose, BepClusterConfig, BepClusterDevice,
    BepCompression, BepDeviceCompression, BepDownloadProgress, BepFileDownloadProgressUpdate,
    BepFileInfo, BepFolder, BepHeader, BepHello, BepIndex, BepMessageType, BepRequest, BepResponse,
    BepVersionCounter, DownloadProgressUpdateType, FolderIndexState, FolderType,
    BEP_BLOCK_HASH_LEN, BEP_BLOCK_SIZE, BEP_DEFAULT_NUM_CONNECTIONS, BEP_DEFAULT_PORT, BEP_MAGIC,
    BEP_MAX_BLOCK_SIZE,
};
pub use cert::DeviceIdentity;
pub use device_id::{DeviceId, DeviceIdParseError};
pub use discovery::{
    announce_on, detect_local_ipv4, is_local_ip, lookup_global_peer, lookup_peer_on,
    parse_global_discovery_json, resolve_endpoint_uri, resolve_peer_tcp_addrs, run_discovery_cycle,
    scan_lan_bep_peers, Announce, AnnounceOutcome, DiscoveryCycleResult, PeerEndpoint,
    DEFAULT_ANNOUNCE_SERVERS, DEFAULT_LOOKUP_SERVER, DISCOVERY_BROADCAST_ADDR, DISCOVERY_MAGIC,
    DISCOVERY_PORT,
};
pub use folder_sync::{IgnoreMatcher, PullPlan};
pub use relay::{
    connect_via_relay, fetch_relay_pool, join_relay_session, RelayListener, RelayMessage,
    RelayResponse, SessionInvitation, DEFAULT_RELAY_POOL,
};
pub use session::{
    accept_and_sync, accept_secondary_cluster_config, connect_and_sync, connect_and_sync_stream,
    PeerHandshakeResult, SecondaryInbox, SessionParams, SyncControl, SyncEvent, SyncSessionStatus,
    SyncTuning,
};
pub use storage::FsSyncStorage;
pub use tls::https::{HttpsClient, HttpsServer};
pub use tls::proxy::HttpProxy;
pub use tls::{accept_bep_tls_stream, probe_bep_peer_device_id, IncomingBepStream, BEP_ALPN};
