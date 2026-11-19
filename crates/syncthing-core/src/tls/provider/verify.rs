//! Signature verification algorithms for `rustls` and `rustls-webpki`.

use der::Decode;
use digest::Digest;
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::signature::Verifier;
use rsa::{pkcs1v15, pss, PublicKey};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{
    alg_id, AlgorithmIdentifier, InvalidSignature, SignatureVerificationAlgorithm,
};
use rustls::SignatureScheme;
use sha2::{Sha256, Sha384, Sha512};

macro_rules! impl_ecdsa_verifier {
    (
        $const_name:ident,
        $struct_name:ident,
        $public_key_algo:expr,
        $signature_alg_id:expr,
        $verifying_key:ty,
        $signature:ty,
        $hash:ty
    ) => {
        #[derive(Debug)]
        struct $struct_name;

        impl SignatureVerificationAlgorithm for $struct_name {
            fn public_key_alg_id(&self) -> AlgorithmIdentifier {
                $public_key_algo
            }

            fn signature_alg_id(&self) -> AlgorithmIdentifier {
                $signature_alg_id
            }

            fn verify_signature(
                &self,
                public_key: &[u8],
                message: &[u8],
                signature: &[u8],
            ) -> Result<(), InvalidSignature> {
                let signature =
                    <$signature>::from_der(signature).map_err(|_| InvalidSignature)?;
                let verifying_key = <$verifying_key>::from_sec1_bytes(public_key)
                    .map_err(|_| InvalidSignature)?;
                let digest = <$hash>::digest(message);
                verifying_key
                    .verify_prehash(&digest, &signature)
                    .map_err(|_| InvalidSignature)
            }
        }

        const $const_name: &dyn SignatureVerificationAlgorithm = &$struct_name;
    };
}

impl_ecdsa_verifier!(
    ECDSA_P256_SHA256,
    EcdsaVerifierP256Sha256,
    alg_id::ECDSA_P256,
    alg_id::ECDSA_SHA256,
    p256::ecdsa::VerifyingKey,
    p256::ecdsa::DerSignature,
    Sha256
);
impl_ecdsa_verifier!(
    ECDSA_P256_SHA384,
    EcdsaVerifierP256Sha384,
    alg_id::ECDSA_P256,
    alg_id::ECDSA_SHA384,
    p256::ecdsa::VerifyingKey,
    p256::ecdsa::DerSignature,
    Sha384
);
impl_ecdsa_verifier!(
    ECDSA_P384_SHA256,
    EcdsaVerifierP384Sha256,
    alg_id::ECDSA_P384,
    alg_id::ECDSA_SHA256,
    p384::ecdsa::VerifyingKey,
    p384::ecdsa::DerSignature,
    Sha256
);
impl_ecdsa_verifier!(
    ECDSA_P384_SHA384,
    EcdsaVerifierP384Sha384,
    alg_id::ECDSA_P384,
    alg_id::ECDSA_SHA384,
    p384::ecdsa::VerifyingKey,
    p384::ecdsa::DerSignature,
    Sha384
);

#[derive(Debug)]
struct Ed25519Verify;

impl SignatureVerificationAlgorithm for Ed25519Verify {
    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ED25519
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ED25519
    }

    fn verify_signature(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), InvalidSignature> {
        use ed25519_dalek::Verifier as _;
        let public_key = public_key.try_into().map_err(|_| InvalidSignature)?;
        let signature =
            ed25519_dalek::Signature::from_slice(signature).map_err(|_| InvalidSignature)?;
        ed25519_dalek::VerifyingKey::from_bytes(public_key)
            .map_err(|_| InvalidSignature)?
            .verify(message, &signature)
            .map_err(|_| InvalidSignature)
    }
}

const ED25519: &dyn SignatureVerificationAlgorithm = &Ed25519Verify;

macro_rules! impl_rsa_verifier {
    (
        $const_name:ident,
        $struct_name:ident,
        $public_key_algo:expr,
        $signature_alg_id:expr,
        $verifying_key:ty,
        $signature:ty
    ) => {
        #[derive(Debug)]
        struct $struct_name;

        impl SignatureVerificationAlgorithm for $struct_name {
            fn public_key_alg_id(&self) -> AlgorithmIdentifier {
                $public_key_algo
            }

            fn signature_alg_id(&self) -> AlgorithmIdentifier {
                $signature_alg_id
            }

            fn verify_signature(
                &self,
                public_key: &[u8],
                message: &[u8],
                signature: &[u8],
            ) -> Result<(), InvalidSignature> {
                let public_key =
                    PublicKey::from_pkcs1_der(public_key).map_err(|_| InvalidSignature)?;
                let signature =
                    <$signature>::try_from(signature).map_err(|_| InvalidSignature)?;
                <$verifying_key>::new(public_key)
                    .verify(message, &signature)
                    .map_err(|_| InvalidSignature)
            }
        }

        const $const_name: &dyn SignatureVerificationAlgorithm = &$struct_name;
    };
}

impl_rsa_verifier!(
    RSA_PKCS1_SHA256,
    RsaVerifierPkcs1Sha256,
    alg_id::RSA_ENCRYPTION,
    alg_id::RSA_PKCS1_SHA256,
    pkcs1v15::VerifyingKey<Sha256>,
    pkcs1v15::Signature
);
impl_rsa_verifier!(
    RSA_PKCS1_SHA384,
    RsaVerifierPkcs1Sha384,
    alg_id::RSA_ENCRYPTION,
    alg_id::RSA_PKCS1_SHA384,
    pkcs1v15::VerifyingKey<Sha384>,
    pkcs1v15::Signature
);
impl_rsa_verifier!(
    RSA_PKCS1_SHA512,
    RsaVerifierPkcs1Sha512,
    alg_id::RSA_ENCRYPTION,
    alg_id::RSA_PKCS1_SHA512,
    pkcs1v15::VerifyingKey<Sha512>,
    pkcs1v15::Signature
);
impl_rsa_verifier!(
    RSA_PSS_SHA256,
    RsaVerifierPssSha256,
    alg_id::RSA_ENCRYPTION,
    alg_id::RSA_PSS_SHA256,
    pss::VerifyingKey<Sha256>,
    pss::Signature
);
impl_rsa_verifier!(
    RSA_PSS_SHA384,
    RsaVerifierPssSha384,
    alg_id::RSA_ENCRYPTION,
    alg_id::RSA_PSS_SHA384,
    pss::VerifyingKey<Sha384>,
    pss::Signature
);
impl_rsa_verifier!(
    RSA_PSS_SHA512,
    RsaVerifierPssSha512,
    alg_id::RSA_ENCRYPTION,
    alg_id::RSA_PSS_SHA512,
    pss::VerifyingKey<Sha512>,
    pss::Signature
);

pub static ALGORITHMS: WebPkiSupportedAlgorithms = WebPkiSupportedAlgorithms {
    all: &[
        ECDSA_P256_SHA256,
        ECDSA_P256_SHA384,
        ECDSA_P384_SHA256,
        ECDSA_P384_SHA384,
        ED25519,
        RSA_PKCS1_SHA256,
        RSA_PKCS1_SHA384,
        RSA_PKCS1_SHA512,
        RSA_PSS_SHA256,
        RSA_PSS_SHA384,
        RSA_PSS_SHA512,
    ],
    mapping: &[
        (
            SignatureScheme::ECDSA_NISTP384_SHA384,
            &[ECDSA_P384_SHA384, ECDSA_P256_SHA384],
        ),
        (
            SignatureScheme::ECDSA_NISTP256_SHA256,
            &[ECDSA_P256_SHA256, ECDSA_P384_SHA256],
        ),
        (SignatureScheme::ED25519, &[ED25519]),
        (SignatureScheme::RSA_PKCS1_SHA256, &[RSA_PKCS1_SHA256]),
        (SignatureScheme::RSA_PKCS1_SHA384, &[RSA_PKCS1_SHA384]),
        (SignatureScheme::RSA_PKCS1_SHA512, &[RSA_PKCS1_SHA512]),
        (SignatureScheme::RSA_PSS_SHA256, &[RSA_PSS_SHA256]),
        (SignatureScheme::RSA_PSS_SHA384, &[RSA_PSS_SHA384]),
        (SignatureScheme::RSA_PSS_SHA512, &[RSA_PSS_SHA512]),
    ],
};
