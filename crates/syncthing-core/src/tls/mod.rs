//! TLS 1.3 transport for BEP/1.0 (`ALPN = "bep/1.0"`), using pure Rust primitives
//! (`rustls` + `rustls-rustcrypto`) compatible with the `armv7-sony-vita-newlibeabihf` target.
//!
//! - [`verifier`] — Syncthing certificate verification (identity = SHA-256 of the peer certificate)
//! - [`client`] — client connections (mutually authenticated streams and anonymous ID probes)
//! - [`polling`] — non-blocking frame polling tolerant of PS Vita `SceNet` timeout codes
//!
//! This module knows nothing about folders or sync logic: see [`crate::session`].

pub mod client;
pub mod polling;
pub mod verifier;

use std::net::TcpStream;

use rustls::{ClientConnection, StreamOwned};

pub use client::probe_bep_peer_device_id;

/// Official ALPN required by Syncthing for BEP/1.0 protocol.
pub const BEP_ALPN: &[u8] = b"bep/1.0";

/// A client-side TLS stream carrying BEP/1.0 frames.
pub type BepTlsStream = StreamOwned<ClientConnection, TcpStream>;
