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
pub mod session;
pub mod storage;
pub mod tls;
pub mod version_vector;

pub use bep::{
    decompress_bep_lz4, read_bep_message, write_bep_message, BepBlockInfo, BepClose,
    BepClusterConfig, BepCompression, BepFileInfo, BepFolder, BepHeader, BepHello, BepIndex,
    BepMessageType, BepRequest, BepResponse, BepVersionCounter, FolderIndexState, BEP_BLOCK_SIZE,
    BEP_DEFAULT_NUM_CONNECTIONS, BEP_DEFAULT_PORT, BEP_MAGIC, BEP_MAX_BLOCK_SIZE,
};
pub use cert::VitaIdentity;
pub use device_id::{DeviceId, DeviceIdParseError};
pub use discovery::{
    detect_local_ipv4, resolve_peer_tcp_addrs, run_discovery_cycle, scan_lan_bep_peers, Announce,
    DiscoveryCycleResult, DISCOVERY_BROADCAST_ADDR, DISCOVERY_MAGIC, DISCOVERY_PORT,
};
pub use folder_sync::PullPlan;
pub use session::{
    connect_and_sync, connect_and_sync_with_control, PeerHandshakeResult, SyncControl, SyncEvent,
    SyncSessionStatus,
};
pub use storage::FsSyncStorage;
pub use tls::{probe_bep_peer_device_id, BEP_ALPN};
