//! Deterministic and random ECDSA P-256 X.509 certificate and DER generator for Syncthing nodes.

use std::io;

use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use p256::pkcs8::{EncodePrivateKey, EncodePublicKey};
use p256::SecretKey;
use sha2::{Digest, Sha256};

use crate::disco::DeviceId;

/// Cryptographic identity of a Syncthing node (X.509 DER certificate + PKCS#8 DER private key + `DeviceId`).
#[derive(Clone)]
pub struct VitaIdentity {
    pub cert_der: Vec<u8>,
    pub key_pkcs8_der: Vec<u8>,
    pub device_id: DeviceId,
}

impl VitaIdentity {
    pub fn from_seed(seed: &[u8; 32]) -> io::Result<Self> {
        let secret_key = SecretKey::from_bytes(seed.into()).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid P-256 seed: {e}"),
            )
        })?;
        let signing_key = SigningKey::from(&secret_key);

        let pkcs8_doc = secret_key.to_pkcs8_der().map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("PKCS#8 encoding error: {e}"))
        })?;
        let key_pkcs8_der = pkcs8_doc.as_bytes().to_vec();

        let spki_doc = secret_key.public_key().to_public_key_der().map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("SPKI encoding error: {e}"))
        })?;
        let spki_der = spki_doc.as_bytes();

        let cert_der = build_self_signed_x509_der(&signing_key, spki_der);

        let mut id_bytes = [0u8; 32];
        let digest = Sha256::digest(&cert_der);
        id_bytes.copy_from_slice(&digest);

        Ok(Self {
            cert_der,
            key_pkcs8_der,
            device_id: DeviceId(id_bytes),
        })
    }
}

fn build_self_signed_x509_der(signing_key: &SigningKey, spki_der: &[u8]) -> Vec<u8> {
    let alg_id = der_seq(&[&[0x06, 0x08, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02]]);
    let version_v3 = &[0xA0, 0x03, 0x02, 0x01, 0x02];
    let serial = &[0x02, 0x01, 0x01];

    let cn_attr = der_seq(&[
        &[0x06, 0x03, 0x55, 0x04, 0x03],
        &der_tlv(0x0C, b"syncthing"),
    ]);
    let rdn_set = der_tlv(0x31, &cn_attr);
    let name = der_seq(&[&rdn_set]);

    let validity = der_seq(&[
        &der_tlv(0x18, b"20240101000000Z"),
        &der_tlv(0x18, b"20491231235959Z"),
    ]);

    let tbs_certificate = der_seq(&[
        version_v3,
        serial,
        &alg_id,
        &name,
        &validity,
        &name,
        spki_der,
    ]);

    let sig: Signature = signing_key.sign(&tbs_certificate);
    let sig_der = sig.to_der();
    let mut bit_string_payload = Vec::with_capacity(1 + sig_der.as_bytes().len());
    bit_string_payload.push(0x00);
    bit_string_payload.extend_from_slice(sig_der.as_bytes());
    let signature_bit_string = der_tlv(0x03, &bit_string_payload);

    der_seq(&[&tbs_certificate, &alg_id, &signature_bit_string])
}

fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let len = content.len();
    let mut out = Vec::with_capacity(4 + len);
    out.push(tag);
    if len < 0x80 {
        out.push(len as u8);
    } else if len <= 0xFF {
        out.push(0x81);
        out.push(len as u8);
    } else {
        out.push(0x82);
        out.push(((len >> 8) & 0xFF) as u8);
        out.push((len & 0xFF) as u8);
    }
    out.extend_from_slice(content);
    out
}

fn der_seq(parts: &[&[u8]]) -> Vec<u8> {
    let total_len: usize = parts.iter().map(|p| p.len()).sum();
    let mut content = Vec::with_capacity(total_len);
    for p in parts {
        content.extend_from_slice(p);
    }
    der_tlv(0x30, &content)
}
