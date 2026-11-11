//! Syncthing peer certificate verification: there is no CA, a peer is identified by the
//! SHA-256 of its self-signed certificate (its `DeviceId`).

use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::device_id::DeviceId;

/// Shared slot where the verifier records the `DeviceId` of the certificate presented by the peer.
pub type ObservedDeviceId = Arc<Mutex<Option<DeviceId>>>;

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
        let digest = Sha256::digest(end_entity.as_ref());
        let mut raw_id = [0u8; 32];
        raw_id.copy_from_slice(&digest);
        let actual_id = DeviceId(raw_id);

        if let Some(expected) = self.expected_device_id {
            if !expected.matches_peer(&actual_id) {
                return Err(TlsError::General(format!(
                    "Peer TLS DeviceId ({actual_id}) != expected DeviceId ({expected})"
                )));
            }
        }

        if let Ok(mut guard) = self.observed_device_id.lock() {
            *guard = Some(actual_id);
        }

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
