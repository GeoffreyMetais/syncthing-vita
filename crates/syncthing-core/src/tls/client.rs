//! TLS 1.3 client connections to Syncthing peers.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::client::WantsClientCert;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, ConfigBuilder, StreamOwned};

use super::verifier::{ObservedDeviceId, SyncthingServerVerifier};
use super::{BepTlsStream, BEP_ALPN};
use crate::cert::DeviceIdentity;
use crate::device_id::DeviceId;

/// TLS server name sent to peers (Syncthing ignores it; identity is the certificate hash).
const SYNCTHING_SERVER_NAME: &str = "syncthing";

/// Starts a client configuration that verifies peers with [`SyncthingServerVerifier`].
fn verified_config_builder(
    expected_device_id: Option<DeviceId>,
) -> io::Result<(
    ConfigBuilder<ClientConfig, WantsClientCert>,
    ObservedDeviceId,
)> {
    let provider: Arc<CryptoProvider> = Arc::new(super::crypto_provider());
    let observed_device_id: ObservedDeviceId = Arc::new(Mutex::new(None));
    let verifier = Arc::new(SyncthingServerVerifier::new(
        Arc::clone(&provider),
        expected_device_id,
        Arc::clone(&observed_device_id),
    ));
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| io::Error::other(format!("TLS version error: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(verifier);
    Ok((builder, observed_device_id))
}

/// Creates a client connection for `config` with the given `alpn` and `server_name`.
fn new_alpn_connection(
    mut config: ClientConfig,
    alpn: &[u8],
    server_name: &str,
) -> io::Result<ClientConnection> {
    if !alpn.is_empty() {
        config.alpn_protocols = vec![alpn.to_vec()];
    }
    let server_name = ServerName::try_from(server_name.to_owned())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("ServerName: {e}")))?;
    ClientConnection::new(Arc::new(config), server_name)
        .map_err(|e| io::Error::other(format!("TLS init failed: {e}")))
}

/// Creates a client connection for `config` with the BEP ALPN.
fn new_bep_connection(config: ClientConfig) -> io::Result<ClientConnection> {
    new_alpn_connection(config, BEP_ALPN, SYNCTHING_SERVER_NAME)
}

/// Wraps an already-connected `TcpStream` in a client-authenticated TLS 1.3 stream
/// (`ALPN = "bep/1.0"`). Used for both direct TCP and relayed BEP sessions.
pub fn wrap_bep_tls_stream(
    identity: &DeviceIdentity,
    tcp: TcpStream,
    expected_device_id: Option<DeviceId>,
    timeout: Duration,
) -> io::Result<(BepTlsStream, ObservedDeviceId)> {
    wrap_tls_stream_with_alpn(identity, tcp, BEP_ALPN, expected_device_id, timeout)
}

/// Wraps an already-connected `TcpStream` in a client-authenticated TLS stream with `alpn`.
pub fn wrap_tls_stream_with_alpn(
    identity: &DeviceIdentity,
    tcp: TcpStream,
    alpn: &[u8],
    expected_device_id: Option<DeviceId>,
    timeout: Duration,
) -> io::Result<(BepTlsStream, ObservedDeviceId)> {
    let _ = tcp.set_nodelay(true);
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;

    let (builder, observed_device_id) = verified_config_builder(expected_device_id)?;
    let cert_chain = vec![CertificateDer::from(identity.cert_der.clone())];
    let priv_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone()));
    let config: ClientConfig = builder
        .with_client_auth_cert(cert_chain, priv_key)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Client cert error: {e}"),
            )
        })?;

    let conn = new_alpn_connection(config, alpn, SYNCTHING_SERVER_NAME)?;
    Ok((
        BepTlsStream::Client(StreamOwned::new(conn, tcp)),
        observed_device_id,
    ))
}

/// Opens a client-authenticated TLS 1.3 stream (`ALPN = "bep/1.0"`) to `peer_addr`.
/// Returns the stream and the slot that receives the peer's `DeviceId` once the handshake ran.
pub fn open_bep_tls_stream(
    identity: &DeviceIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    timeout: Duration,
) -> io::Result<(BepTlsStream, ObservedDeviceId)> {
    let tcp = TcpStream::connect_timeout(&peer_addr, timeout)?;
    wrap_bep_tls_stream(identity, tcp, expected_device_id, timeout)
}

/// Opens a client-authenticated TLS stream with a custom `alpn` (e.g. `b"bep-relay"`).
pub fn open_tls_stream_with_alpn(
    identity: &DeviceIdentity,
    peer_addr: SocketAddr,
    alpn: &[u8],
    expected_device_id: Option<DeviceId>,
    timeout: Duration,
) -> io::Result<(BepTlsStream, ObservedDeviceId)> {
    let tcp = TcpStream::connect_timeout(&peer_addr, timeout)?;
    wrap_tls_stream_with_alpn(identity, tcp, alpn, expected_device_id, timeout)
}

/// Probes a TCP endpoint with a TLS 1.3 (`bep/1.0` ALPN) handshake and returns its Syncthing `DeviceId`
/// (SHA-256 of the server's X.509 certificate) if it is a valid Syncthing BEP peer.
pub fn probe_bep_peer_device_id(peer_addr: SocketAddr, timeout: Duration) -> Option<DeviceId> {
    let mut tcp = TcpStream::connect_timeout(&peer_addr, timeout).ok()?;
    let _ = tcp.set_read_timeout(Some(timeout));
    let _ = tcp.set_write_timeout(Some(timeout));

    let (builder, observed_device_id) = verified_config_builder(None).ok()?;
    let mut conn = new_bep_connection(builder.with_no_client_auth()).ok()?;

    while conn.is_handshaking() {
        if conn.complete_io(&mut tcp).is_err() {
            break;
        }
    }

    let id = observed_device_id.lock().ok().and_then(|g| *g);
    id
}
