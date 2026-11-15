//! Syncthing peer certificate verification: there is no CA, a peer is identified by the
//! SHA-256 of its self-signed certificate (its `DeviceId`).

use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, Error as TlsError, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::device_id::DeviceId;

/// Shared slot where the verifier records the `DeviceId` of the certificate presented by the peer.
pub type ObservedDeviceId = Arc<Mutex<Option<DeviceId>>>;

/// Identifies a peer by the SHA-256 of its certificate: fails if it does not match
/// `expected` (prefix matching is supported for shortened IDs), records it in `observed`.
fn check_peer_certificate(
    end_entity: &CertificateDer<'_>,
    expected: Option<DeviceId>,
    observed: &ObservedDeviceId,
) -> Result<(), TlsError> {
    let digest = Sha256::digest(end_entity.as_ref());
    let mut raw_id = [0u8; 32];
    raw_id.copy_from_slice(&digest);
    let actual_id = DeviceId(raw_id);

    if let Some(expected) = expected {
        if !expected.matches_peer(&actual_id) {
            return Err(TlsError::General(format!(
                "Peer TLS DeviceId ({actual_id}) != expected DeviceId ({expected})"
            )));
        }
    }
    if let Ok(mut guard) = observed.lock() {
        *guard = Some(actual_id);
    }
    Ok(())
}

/// Accepts any self-signed peer certificate, optionally pinned to an expected `DeviceId`
/// (prefix matching is supported for shortened, user-entered IDs), and records the observed ID.
#[derive(Debug)]
pub struct SyncthingServerVerifier {
    provider: Arc<CryptoProvider>,
    expected_device_id: Option<DeviceId>,
    observed_device_id: ObservedDeviceId,
}

impl SyncthingServerVerifier {
    pub fn new(
        provider: Arc<CryptoProvider>,
        expected_device_id: Option<DeviceId>,
        observed_device_id: ObservedDeviceId,
    ) -> Self {
        Self {
            provider,
            expected_device_id,
            observed_device_id,
        }
    }
}

impl ServerCertVerifier for SyncthingServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        check_peer_certificate(
            end_entity,
            self.expected_device_id,
            &self.observed_device_id,
        )?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Server side of [`SyncthingServerVerifier`]: requires a client certificate (Syncthing peers
/// always present one), accepts any self-signed one and records its `DeviceId`. Whether that
/// device is welcome is the application's decision, once the handshake told it who it is.
#[derive(Debug)]
pub struct SyncthingClientVerifier {
    provider: Arc<CryptoProvider>,
    observed_device_id: ObservedDeviceId,
}

impl SyncthingClientVerifier {
    pub fn new(provider: Arc<CryptoProvider>, observed_device_id: ObservedDeviceId) -> Self {
        Self {
            provider,
            observed_device_id,
        }
    }
}

impl ClientCertVerifier for SyncthingClientVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, TlsError> {
        check_peer_certificate(end_entity, None, &self.observed_device_id)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Server verifier of [`super::https`]: a certificate whose SHA-256 is the pinned `DeviceId`
/// is accepted as is; any other one goes through the regular public-root verification.
#[derive(Debug)]
pub struct PinnedOrWebPkiVerifier {
    pinned_id: Option<DeviceId>,
    webpki: Arc<WebPkiServerVerifier>,
}

impl PinnedOrWebPkiVerifier {
    pub fn new(pinned_id: Option<DeviceId>, webpki: Arc<WebPkiServerVerifier>) -> Self {
        Self { pinned_id, webpki }
    }
}

impl ServerCertVerifier for PinnedOrWebPkiVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        if let Some(pinned) = self.pinned_id {
            let digest = Sha256::digest(end_entity.as_ref());
            if digest.as_slice() == pinned.0 {
                return Ok(ServerCertVerified::assertion());
            }
        }
        self.webpki
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.webpki.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.webpki.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.webpki.supported_verify_schemes()
    }
}
