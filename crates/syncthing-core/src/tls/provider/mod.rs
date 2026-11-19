//! Pure-Rust `rustls` [`CryptoProvider`] backed by RustCrypto primitives.

mod aead;
mod hash;
mod hmac;
mod kx;
mod sign;
mod verify;

use std::sync::Arc;

use rustls::crypto::tls12::PrfUsingHmac;
use rustls::crypto::tls13::HkdfUsingHmac;
use rustls::crypto::{
    CipherSuiteCommon, CryptoProvider, GetRandomFailed, KeyExchangeAlgorithm, KeyProvider,
    SecureRandom,
};
use rustls::pki_types::PrivateKeyDer;
use rustls::sign::SigningKey;
use rustls::{
    CipherSuite, Error, SignatureScheme, SupportedCipherSuite, Tls12CipherSuite, Tls13CipherSuite,
};

#[derive(Debug)]
struct Provider;

/// Adapter bridging `rand_core 0.6` (`OsRng`) to `rand_core 0.10` (`TryRng` / `CryptoRng`)
/// used by `p256 0.14` and `p384 0.14`.
struct OsRngAdapter;

impl p256::elliptic_curve::rand_core::TryRng for OsRngAdapter {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        use rand_core::RngCore as _;
        Ok(rand_core::OsRng.next_u32())
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        use rand_core::RngCore as _;
        Ok(rand_core::OsRng.next_u64())
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
        use rand_core::RngCore as _;
        rand_core::OsRng.fill_bytes(dest);
        Ok(())
    }
}

impl p256::elliptic_curve::rand_core::TryCryptoRng for OsRngAdapter {}

impl SecureRandom for Provider {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), GetRandomFailed> {
        use rand_core::RngCore as _;
        rand_core::OsRng
            .try_fill_bytes(bytes)
            .map_err(|_| GetRandomFailed)
    }
}

impl KeyProvider for Provider {
    fn load_private_key(
        &self,
        key_der: PrivateKeyDer<'static>,
    ) -> Result<Arc<dyn SigningKey>, Error> {
        sign::any_supported_type(&key_der)
    }
}

const TLS12_ECDSA_SCHEMES: [SignatureScheme; 4] = [
    SignatureScheme::ECDSA_NISTP256_SHA256,
    SignatureScheme::ECDSA_NISTP384_SHA384,
    SignatureScheme::ECDSA_NISTP521_SHA512,
    SignatureScheme::ED25519,
];

const TLS12_RSA_SCHEMES: [SignatureScheme; 6] = [
    SignatureScheme::RSA_PKCS1_SHA256,
    SignatureScheme::RSA_PKCS1_SHA384,
    SignatureScheme::RSA_PKCS1_SHA512,
    SignatureScheme::RSA_PSS_SHA256,
    SignatureScheme::RSA_PSS_SHA384,
    SignatureScheme::RSA_PSS_SHA512,
];

const TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        kx: KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_ECDSA_SCHEMES,
        aead_alg: &aead::gcm::Tls12Aes128Gcm,
        prf_provider: &PrfUsingHmac(hmac::SHA256),
    });

const TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
            hash_provider: hash::SHA384,
            confidentiality_limit: u64::MAX,
        },
        kx: KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_ECDSA_SCHEMES,
        aead_alg: &aead::gcm::Tls12Aes256Gcm,
        prf_provider: &PrfUsingHmac(hmac::SHA384),
    });

const TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        kx: KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_ECDSA_SCHEMES,
        aead_alg: &aead::chacha20::Chacha20Poly1305,
        prf_provider: &PrfUsingHmac(hmac::SHA256),
    });

const TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        kx: KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_RSA_SCHEMES,
        aead_alg: &aead::gcm::Tls12Aes128Gcm,
        prf_provider: &PrfUsingHmac(hmac::SHA256),
    });

const TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
            hash_provider: hash::SHA384,
            confidentiality_limit: u64::MAX,
        },
        kx: KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_RSA_SCHEMES,
        aead_alg: &aead::gcm::Tls12Aes256Gcm,
        prf_provider: &PrfUsingHmac(hmac::SHA384),
    });

const TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls12(&Tls12CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        kx: KeyExchangeAlgorithm::ECDHE,
        sign: &TLS12_RSA_SCHEMES,
        aead_alg: &aead::chacha20::Chacha20Poly1305,
        prf_provider: &PrfUsingHmac(hmac::SHA256),
    });

const TLS13_AES_128_GCM_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls13(&Tls13CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS13_AES_128_GCM_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        hkdf_provider: &HkdfUsingHmac(hmac::SHA256),
        aead_alg: &aead::gcm::Tls13Aes128Gcm,
        quic: None,
    });

const TLS13_AES_256_GCM_SHA384: SupportedCipherSuite =
    SupportedCipherSuite::Tls13(&Tls13CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS13_AES_256_GCM_SHA384,
            hash_provider: hash::SHA384,
            confidentiality_limit: u64::MAX,
        },
        hkdf_provider: &HkdfUsingHmac(hmac::SHA384),
        aead_alg: &aead::gcm::Tls13Aes256Gcm,
        quic: None,
    });

const TLS13_CHACHA20_POLY1305_SHA256: SupportedCipherSuite =
    SupportedCipherSuite::Tls13(&Tls13CipherSuite {
        common: CipherSuiteCommon {
            suite: CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
            hash_provider: hash::SHA256,
            confidentiality_limit: u64::MAX,
        },
        hkdf_provider: &HkdfUsingHmac(hmac::SHA256),
        aead_alg: &aead::chacha20::Chacha20Poly1305,
        quic: None,
    });

static ALL_CIPHER_SUITES: &[SupportedCipherSuite] = &[
    TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
    TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
    TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
    TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
    TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
    TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
    TLS13_AES_128_GCM_SHA256,
    TLS13_AES_256_GCM_SHA384,
    TLS13_CHACHA20_POLY1305_SHA256,
];

/// Returns the pure-Rust `rustls` [`CryptoProvider`] used by `syncthing-core`.
pub fn crypto_provider() -> CryptoProvider {
    CryptoProvider {
        cipher_suites: ALL_CIPHER_SUITES.to_vec(),
        kx_groups: kx::ALL_KX_GROUPS.to_vec(),
        signature_verification_algorithms: verify::ALGORITHMS,
        secure_random: &Provider,
        key_provider: &Provider,
    }
}
