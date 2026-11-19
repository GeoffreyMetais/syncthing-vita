//! Private key loading and TLS handshake signing for ECDSA (P-256, P-384) and Ed25519.

use std::marker::PhantomData;
use std::sync::Arc;

use p256::ecdsa::signature::RandomizedSigner;
use pkcs8::DecodePrivateKey;
use rustls::pki_types::PrivateKeyDer;
use rustls::sign::{Signer, SigningKey};
use rustls::{Error, SignatureAlgorithm, SignatureScheme};

#[derive(Debug)]
struct EcdsaSigner<S, T>
where
    T: RandomizedSigner<S>,
{
    _marker: PhantomData<S>,
    key: Arc<T>,
    scheme: SignatureScheme,
}

impl<S, T> Signer for EcdsaSigner<S, T>
where
    S: AsRef<[u8]> + Send + Sync + std::fmt::Debug,
    T: RandomizedSigner<S> + Send + Sync + std::fmt::Debug,
{
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let mut rng = super::OsRngAdapter;
        self.key
            .try_sign_with_rng(&mut rng, message)
            .map_err(|_| Error::General("signing failed".into()))
            .map(|sig: S| sig.as_ref().to_vec())
    }

    fn scheme(&self) -> SignatureScheme {
        self.scheme
    }
}

macro_rules! impl_ecdsa_signing_key {
    ($ty:ident, $scheme:expr, $secret_key:ty, $signing_key:ty, $signature:ty) => {
        #[derive(Debug)]
        struct $ty {
            key: Arc<$signing_key>,
            scheme: SignatureScheme,
        }

        impl TryFrom<&PrivateKeyDer<'_>> for $ty {
            type Error = Error;

            fn try_from(value: &PrivateKeyDer<'_>) -> Result<Self, Self::Error> {
                let pkey = match value {
                    PrivateKeyDer::Pkcs8(der) => {
                        <$signing_key>::from_pkcs8_der(der.secret_pkcs8_der())
                            .map_err(|e| format!("failed to decode PKCS#8 key: {e}"))
                    }
                    PrivateKeyDer::Sec1(sec1) => {
                        <$secret_key>::from_sec1_der(sec1.secret_sec1_der())
                            .map(|sk| <$signing_key>::from(&sk))
                            .map_err(|e| format!("failed to decode SEC1 key: {e}"))
                    }
                    PrivateKeyDer::Pkcs1(_) => Err("ECDSA does not support PKCS#1 key".into()),
                    _ => Err("unsupported private key format".into()),
                };
                pkey.map(|kp| Self {
                    key: Arc::new(kp),
                    scheme: $scheme,
                })
                .map_err(Error::General)
            }
        }

        impl SigningKey for $ty {
            fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
                if offered.contains(&self.scheme) {
                    Some(Box::new(EcdsaSigner::<$signature, _> {
                        _marker: PhantomData,
                        key: Arc::clone(&self.key),
                        scheme: self.scheme,
                    }))
                } else {
                    None
                }
            }

            fn algorithm(&self) -> SignatureAlgorithm {
                SignatureAlgorithm::ECDSA
            }
        }
    };
}

impl_ecdsa_signing_key!(
    EcdsaSigningKeyP256,
    SignatureScheme::ECDSA_NISTP256_SHA256,
    p256::SecretKey,
    p256::ecdsa::SigningKey,
    p256::ecdsa::DerSignature
);
impl_ecdsa_signing_key!(
    EcdsaSigningKeyP384,
    SignatureScheme::ECDSA_NISTP384_SHA384,
    p384::SecretKey,
    p384::ecdsa::SigningKey,
    p384::ecdsa::DerSignature
);

#[derive(Debug)]
struct Ed25519SigningKey {
    key: Arc<ed25519_dalek::SigningKey>,
}

impl TryFrom<&PrivateKeyDer<'_>> for Ed25519SigningKey {
    type Error = Error;

    fn try_from(value: &PrivateKeyDer<'_>) -> Result<Self, Self::Error> {
        use ed25519_dalek::pkcs8::DecodePrivateKey as _;
        match value {
            PrivateKeyDer::Pkcs8(der) => {
                ed25519_dalek::SigningKey::from_pkcs8_der(der.secret_pkcs8_der())
                    .map(|kp| Self { key: Arc::new(kp) })
                    .map_err(|e| Error::General(format!("failed to decode Ed25519 key: {e}")))
            }
            _ => Err(Error::General("Ed25519 requires PKCS#8 private key".into())),
        }
    }
}

impl SigningKey for Ed25519SigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        if offered.contains(&SignatureScheme::ED25519) {
            Some(Box::new(Ed25519Signer {
                key: Arc::clone(&self.key),
            }))
        } else {
            None
        }
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ED25519
    }
}

#[derive(Debug)]
struct Ed25519Signer {
    key: Arc<ed25519_dalek::SigningKey>,
}

impl Signer for Ed25519Signer {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        use ed25519_dalek::Signer as _;
        Ok(self.key.sign(message).to_bytes().to_vec())
    }

    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

/// Decodes an ECDSA (P-256 / P-384) or Ed25519 private key from DER.
pub fn any_supported_type(der: &PrivateKeyDer<'_>) -> Result<Arc<dyn SigningKey>, Error> {
    EcdsaSigningKeyP256::try_from(der)
        .map(|k| Arc::new(k) as Arc<dyn SigningKey>)
        .or_else(|_| EcdsaSigningKeyP384::try_from(der).map(|k| Arc::new(k) as _))
        .or_else(|_| Ed25519SigningKey::try_from(der).map(|k| Arc::new(k) as _))
}
