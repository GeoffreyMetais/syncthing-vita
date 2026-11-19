//! TLS 1.3 server connections: a peer that dialed us (incoming TCP connection, or a relay
//! session we were invited to with `ServerSocket` set).

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use super::verifier::{ObservedDeviceId, SyncthingClientVerifier};
use super::{BepTlsStream, BEP_ALPN};
use crate::cert::DeviceIdentity;
use crate::device_id::DeviceId;

/// A peer connection accepted and authenticated: the TLS handshake is complete, nothing else
/// has been exchanged yet. The application decides from [`Self::peer_device_id`] whether to
/// run a session ([`crate::session::accept_and_sync`]) or drop the connection.
pub struct IncomingBepStream {
    pub(crate) stream: BepTlsStream,
    peer_device_id: DeviceId,
}

impl IncomingBepStream {
    /// The `DeviceId` proven by the peer's certificate.
    pub fn peer_device_id(&self) -> DeviceId {
        self.peer_device_id
    }
}

/// Server configuration: our certificate, a mandatory client certificate (recorded in the
/// returned slot) and the BEP ALPN.
fn bep_server_config(identity: &DeviceIdentity) -> io::Result<(ServerConfig, ObservedDeviceId)> {
    let provider: Arc<CryptoProvider> = Arc::new(super::crypto_provider());
    let observed: ObservedDeviceId = Arc::new(Mutex::new(None));
    let verifier = Arc::new(SyncthingClientVerifier::new(
        Arc::clone(&provider),
        Arc::clone(&observed),
    ));
    let cert_chain = vec![CertificateDer::from(identity.cert_der.clone())];
    let priv_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone()));
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| io::Error::other(format!("TLS version error: {e}")))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(cert_chain, priv_key)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Server cert error: {e}"),
            )
        })?;
    config.alpn_protocols = vec![BEP_ALPN.to_vec()];
    Ok((config, observed))
}

/// Runs the server side of the TLS handshake on `tcp` and returns the authenticated stream.
/// Fails if the handshake fails, takes longer than `timeout` in total (a peer trickling bytes
/// cannot hold the connection longer), or the peer presented no certificate. The returned
/// stream's socket has `timeout` as its read and write timeouts.
pub fn accept_bep_tls_stream(
    identity: &DeviceIdentity,
    mut tcp: TcpStream,
    timeout: Duration,
) -> io::Result<IncomingBepStream> {
    let _ = tcp.set_nodelay(true);

    let (config, observed) = bep_server_config(identity)?;
    let mut conn = ServerConnection::new(Arc::new(config))
        .map_err(|e| io::Error::other(format!("TLS init failed: {e}")))?;
    let mut handshake_io = DeadlineIo {
        tcp: &mut tcp,
        deadline: Instant::now() + timeout,
    };
    while conn.is_handshaking() {
        conn.complete_io(&mut handshake_io)?;
    }
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;

    let peer_device_id =
        observed.lock().ok().and_then(|g| *g).ok_or_else(|| {
            io::Error::new(io::ErrorKind::PermissionDenied, "No client certificate")
        })?;
    Ok(IncomingBepStream {
        stream: BepTlsStream::Server(StreamOwned::new(conn, tcp)),
        peer_device_id,
    })
}

/// The socket during the handshake: every read and write is bounded by the time left before
/// `deadline` (a per-operation timeout alone would restart with each byte received).
struct DeadlineIo<'a> {
    tcp: &'a mut TcpStream,
    deadline: Instant,
}

impl DeadlineIo<'_> {
    /// Time left before the deadline (an error once it passed).
    fn time_left(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "TLS handshake timed out"))
    }
}

impl Read for DeadlineIo<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.time_left()?;
        self.tcp.set_read_timeout(Some(left))?;
        self.tcp.read(buf)
    }
}

impl Write for DeadlineIo<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let left = self.time_left()?;
        self.tcp.set_write_timeout(Some(left))?;
        self.tcp.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}
