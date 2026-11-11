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
use crate::cert::VitaIdentity;
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
    let provider: Arc<CryptoProvider> = Arc::new(rustls_rustcrypto::provider());
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

/// Creates a client connection for `config` with the BEP ALPN.
fn new_bep_connection(mut config: ClientConfig) -> io::Result<ClientConnection> {
    config.alpn_protocols = vec![BEP_ALPN.to_vec()];
    let server_name = ServerName::try_from(SYNCTHING_SERVER_NAME)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("ServerName: {e}")))?;
    ClientConnection::new(Arc::new(config), server_name)
        .map_err(|e| io::Error::other(format!("TLS init failed: {e}")))
}

/// Opens a client-authenticated TLS 1.3 stream (`ALPN = "bep/1.0"`) to `peer_addr`.
/// Returns the stream and the slot that receives the peer's `DeviceId` once the handshake ran.
pub fn open_bep_tls_stream(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    timeout: Duration,
) -> io::Result<(BepTlsStream, ObservedDeviceId)> {
    let tcp = TcpStream::connect_timeout(&peer_addr, timeout)?;
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

    let conn = new_bep_connection(config)?;
    Ok((StreamOwned::new(conn, tcp), observed_device_id))
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
