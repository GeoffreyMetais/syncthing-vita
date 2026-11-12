//! Implementation of Syncthing's **Block Exchange Protocol v1 (BEP/1.0)** wire format.
//! Specification: https://docs.syncthing.net/specs/bep-v1.html
//!
//! This module is a pure codec layer (no sockets, no filesystem). One file per message family:
//! - [`header`] — frame `Header` (`MessageType`, `MessageCompression`)
//! - [`framing`] — reading / writing complete post-`Hello` frames
//! - [`lz4`] — LZ4 block decompression of compressed frames
//! - [`hello`] — pre-authentication `Hello` handshake
//! - [`cluster_config`] — `ClusterConfig` (with `index_id` and `max_sequence`)
//! - [`index`] — `Index` / `IndexUpdate` (files, BEP `Vector` / `Counter`, blocks)
//! - [`request`] — block `Request` / `Response`
//! - [`close`] — `Close`

pub mod close;
pub mod cluster_config;
pub mod framing;
pub mod header;
pub mod hello;
pub mod index;
pub mod lz4;
pub mod request;

pub use close::BepClose;
pub use cluster_config::{BepClusterConfig, BepFolder, FolderIndexState};
pub use framing::{read_bep_message, read_bep_message_after_first_byte, write_bep_message};
pub use header::{BepCompression, BepHeader, BepMessageType};
pub use hello::BepHello;
pub use index::{BepBlockInfo, BepFileInfo, BepIndex, BepVersionCounter};
pub use lz4::decompress_bep_lz4;
pub use request::{BepRequest, BepResponse};

/// BEP magic number (`0x2EA7D90B`), shared with Local Discovery.
pub const BEP_MAGIC: u32 = crate::discovery::DISCOVERY_MAGIC;

/// Default TCP port for Syncthing BEP protocol
pub const BEP_DEFAULT_PORT: u16 = 22000;

/// Standard block size for Syncthing BEP (128 KiB)
pub const BEP_BLOCK_SIZE: usize = 128 * 1024;

/// Largest block a peer may announce or request (Syncthing's maximum block size, 16 MiB).
/// Sizes come from the network: anything larger is rejected before allocating memory for it.
pub const BEP_MAX_BLOCK_SIZE: usize = 16 * 1024 * 1024;

/// Default number of parallel BEP connections advertised to Syncthing v2.0+ peers
/// (`1` primary metadata connection + `2` secondary data connections = `1+2`).
pub const BEP_DEFAULT_NUM_CONNECTIONS: i32 = 3;
