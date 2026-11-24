//! TLS 1.3 transport for BEP/1.0 (`ALPN = "bep/1.0"`), using pure Rust primitives
//! (`rustls` + RustCrypto provider) compatible with the `armv7-sony-vita-newlibeabihf` target.
//!
//! - [`verifier`] — Syncthing certificate verification (identity = SHA-256 of the peer certificate)
//! - [`client`] — client connections (mutually authenticated streams and anonymous ID probes)
//! - [`proxy`] — HTTP proxy tunnels (`CONNECT`) for [`https`]
//! - [`https`] — HTTPS to discovery servers / relay pool (pinned or public certificates)
//! - [`server`] — server connections (incoming peers, relay invitations with `ServerSocket`)
//! - [`polling`] — non-blocking frame polling tolerant of PS Vita `SceNet` timeout codes
//!
//! This module knows nothing about folders or sync logic: see [`crate::session`].

pub mod client;
pub mod https;
pub mod polling;
pub mod provider;
pub mod proxy;
pub mod server;
pub mod verifier;

use std::io::{self, Read, Write};
use std::net::TcpStream;

use rustls::{ClientConnection, ServerConnection, StreamOwned};

pub use client::probe_bep_peer_device_id;
pub use provider::crypto_provider;
pub use server::{accept_bep_tls_stream, IncomingBepStream};

/// Official ALPN required by Syncthing for BEP/1.0 protocol.
pub const BEP_ALPN: &[u8] = b"bep/1.0";

/// A TLS stream carrying BEP/1.0 frames. After the handshake both sides of a BEP session
/// behave the same: only who dialed (TLS client) and who accepted (TLS server) differs.
pub enum BepTlsStream {
    /// We opened the connection (or joined a relay session as the initiator).
    Client(StreamOwned<ClientConnection, TcpStream>),
    /// The peer opened the connection (incoming TCP, or a relay invitation with `ServerSocket`).
    Server(StreamOwned<ServerConnection, TcpStream>),
}

impl BepTlsStream {
    /// The underlying socket (timeouts, shutdown).
    pub fn get_mut(&mut self) -> &mut TcpStream {
        match self {
            Self::Client(s) => s.get_mut(),
            Self::Server(s) => s.get_mut(),
        }
    }

    /// The underlying socket, read-only.
    pub fn get_ref(&self) -> &TcpStream {
        match self {
            Self::Client(s) => s.get_ref(),
            Self::Server(s) => s.get_ref(),
        }
    }

    /// `true` when the peer opened the connection.
    pub fn is_server(&self) -> bool {
        matches!(self, Self::Server(_))
    }

    /// Sends a TLS `close_notify` alert and flushes it to the underlying socket so the peer sees a
    /// clean TLS shutdown if it reads past the final BEP `Close` frame.
    pub fn send_close_notify(&mut self) {
        match self {
            Self::Client(s) => {
                s.conn.send_close_notify();
                let _ = s.flush();
            }
            Self::Server(s) => {
                s.conn.send_close_notify();
                let _ = s.flush();
            }
        }
    }
}

impl Read for BepTlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Client(s) => s.read(buf),
            Self::Server(s) => s.read(buf),
        }
    }
}

impl Write for BepTlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Client(s) => s.write(buf),
            Self::Server(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Client(s) => s.flush(),
            Self::Server(s) => s.flush(),
        }
    }
}
