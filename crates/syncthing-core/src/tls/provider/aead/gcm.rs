//! AES-GCM AEAD algorithms for TLS 1.3 and TLS 1.2.

use aead::{AeadCore, AeadInPlace, KeyInit, KeySizeUser};
use aes_gcm::aead::generic_array::typenum::Unsigned;
use rustls::crypto::cipher::{
    self, AeadKey, InboundOpaqueMessage, InboundPlainMessage, Iv, KeyBlockShape, MessageDecrypter,
    MessageEncrypter, OutboundOpaqueMessage, OutboundPlainMessage, PrefixedPayload,
    Tls12AeadAlgorithm, Tls13AeadAlgorithm, UnsupportedOperationError,
};
use rustls::{ConnectionTrafficSecrets, ContentType, Error, ProtocolVersion};

use super::{DecryptBufferAdapter, EncryptBufferAdapter};

const TLS12_GCM_EXPLICIT_NONCE_LEN: usize = 8;
const TLS12_GCM_OVERHEAD: usize = TLS12_GCM_EXPLICIT_NONCE_LEN + 16;

macro_rules! impl_gcm_tls13 {
    ($alg_ty:ident, $cipher_ty:ident, $aead:ty, $secrets_variant:ident, $overhead:expr) => {
        pub struct $alg_ty;

        impl Tls13AeadAlgorithm for $alg_ty {
            fn encrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageEncrypter> {
                Box::new($cipher_ty(
                    <$aead>::new_from_slice(key.as_ref()).expect("key should be valid"),
                    iv,
                ))
            }

            fn decrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageDecrypter> {
                Box::new($cipher_ty(
                    <$aead>::new_from_slice(key.as_ref()).expect("key should be valid"),
                    iv,
                ))
            }

            fn key_len(&self) -> usize {
                <$aead>::key_size()
            }

            fn extract_keys(
                &self,
                key: AeadKey,
                iv: Iv,
            ) -> Result<ConnectionTrafficSecrets, UnsupportedOperationError> {
                Ok(ConnectionTrafficSecrets::$secrets_variant { key, iv })
            }
        }

        struct $cipher_ty($aead, Iv);

        impl MessageEncrypter for $cipher_ty {
            fn encrypt(
                &mut self,
                m: OutboundPlainMessage<'_>,
                seq: u64,
            ) -> Result<OutboundOpaqueMessage, Error> {
                let total_len = self.encrypted_payload_len(m.payload.len());
                let mut payload = PrefixedPayload::with_capacity(total_len);

                let nonce = cipher::Nonce::new(&self.1, seq).0;
                let aad = cipher::make_tls13_aad(total_len);
                payload.extend_from_chunks(&m.payload);
                payload.extend_from_slice(&m.typ.to_array());

                self.0
                    .encrypt_in_place(&nonce.into(), &aad, &mut EncryptBufferAdapter(&mut payload))
                    .map_err(|_| Error::EncryptError)
                    .map(|()| {
                        OutboundOpaqueMessage::new(
                            ContentType::ApplicationData,
                            ProtocolVersion::TLSv1_2,
                            payload,
                        )
                    })
            }

            fn encrypted_payload_len(&self, payload_len: usize) -> usize {
                payload_len + 1 + $overhead
            }
        }

        impl MessageDecrypter for $cipher_ty {
            fn decrypt<'a>(
                &mut self,
                mut m: InboundOpaqueMessage<'a>,
                seq: u64,
            ) -> Result<InboundPlainMessage<'a>, Error> {
                let payload = &mut m.payload;
                let nonce = cipher::Nonce::new(&self.1, seq).0;
                let aad = cipher::make_tls13_aad(payload.len());

                self.0
                    .decrypt_in_place(&nonce.into(), &aad, &mut DecryptBufferAdapter(payload))
                    .map_err(|_| Error::DecryptError)?;

                m.into_tls13_unpadded_message()
            }
        }
    };
}

macro_rules! impl_gcm_tls12 {
    ($alg_ty:ident, $enc_ty:ident, $dec_ty:ident, $aead:ty, $secrets_variant:ident) => {
        pub struct $alg_ty;

        impl Tls12AeadAlgorithm for $alg_ty {
            fn encrypter(
                &self,
                key: AeadKey,
                write_iv: &[u8],
                explicit: &[u8],
            ) -> Box<dyn MessageEncrypter> {
                let mut iv = [0u8; 12];
                iv[..4].copy_from_slice(write_iv);
                iv[4..].copy_from_slice(explicit);
                Box::new($enc_ty(
                    <$aead>::new_from_slice(key.as_ref()).expect("key should be valid"),
                    iv,
                ))
            }

            fn decrypter(&self, dec_key: AeadKey, dec_iv: &[u8]) -> Box<dyn MessageDecrypter> {
                Box::new($dec_ty(
                    <$aead>::new_from_slice(dec_key.as_ref()).expect("key should be valid"),
                    dec_iv.try_into().expect("iv should be 4 bytes"),
                ))
            }

            fn key_block_shape(&self) -> KeyBlockShape {
                KeyBlockShape {
                    enc_key_len: <$aead>::key_size(),
                    fixed_iv_len: 4,
                    explicit_nonce_len: TLS12_GCM_EXPLICIT_NONCE_LEN,
                }
            }

            fn extract_keys(
                &self,
                key: AeadKey,
                iv: &[u8],
                _explicit: &[u8],
            ) -> Result<ConnectionTrafficSecrets, UnsupportedOperationError> {
                Ok(ConnectionTrafficSecrets::$secrets_variant {
                    key,
                    iv: Iv::new(iv[..].try_into().expect("iv should be 12 bytes")),
                })
            }
        }

        struct $enc_ty($aead, [u8; 12]);

        impl MessageEncrypter for $enc_ty {
            fn encrypt(
                &mut self,
                m: OutboundPlainMessage<'_>,
                seq: u64,
            ) -> Result<OutboundOpaqueMessage, Error> {
                let total_len = self.encrypted_payload_len(m.payload.len());
                let mut payload = PrefixedPayload::with_capacity(total_len);

                let nonce = cipher::Nonce::new(&self.1.into(), seq).0;
                let aad = cipher::make_tls12_aad(seq, m.typ, m.version, m.payload.len());
                payload.extend_from_slice(&nonce.as_ref()[4..]);
                payload.extend_from_chunks(&m.payload);

                self.0
                    .encrypt_in_place_detached(
                        &nonce.into(),
                        &aad,
                        &mut payload.as_mut()[TLS12_GCM_EXPLICIT_NONCE_LEN..],
                    )
                    .map(|tag| payload.extend(tag.as_ref() as &[u8]))
                    .map_err(|_| Error::EncryptError)
                    .map(|()| OutboundOpaqueMessage::new(m.typ, m.version, payload))
            }

            fn encrypted_payload_len(&self, payload_len: usize) -> usize {
                payload_len + TLS12_GCM_EXPLICIT_NONCE_LEN + <$aead as AeadCore>::TagSize::USIZE
            }
        }

        struct $dec_ty($aead, [u8; 4]);

        impl MessageDecrypter for $dec_ty {
            fn decrypt<'a>(
                &mut self,
                mut m: InboundOpaqueMessage<'a>,
                seq: u64,
            ) -> Result<InboundPlainMessage<'a>, Error> {
                type TagSize = <$aead as AeadCore>::TagSize;

                let payload = &m.payload;
                if payload.len() < TLS12_GCM_OVERHEAD {
                    return Err(Error::DecryptError);
                }

                let nonce: aead::Nonce<$aead> = {
                    let mut nonce = [0u8; 12];
                    nonce[..4].copy_from_slice(&self.1);
                    nonce[4..].copy_from_slice(&payload[..TLS12_GCM_EXPLICIT_NONCE_LEN]);
                    nonce.into()
                };

                let aad = cipher::make_tls12_aad(
                    seq,
                    m.typ,
                    m.version,
                    payload.len() - TLS12_GCM_OVERHEAD,
                );

                let payload = &mut m.payload;
                let tag_pos = {
                    let payload = &mut payload[TLS12_GCM_EXPLICIT_NONCE_LEN..];
                    let tag_pos = payload.len() - TagSize::to_usize();
                    let (msg, tag) = payload.split_at_mut(tag_pos);

                    let tag = aes_gcm::Tag::<TagSize>::from_slice(tag);
                    self.0
                        .decrypt_in_place_detached(&nonce, &aad, msg, tag)
                        .map_err(|_| Error::DecryptError)?;
                    tag_pos
                };

                payload.rotate_left(TLS12_GCM_EXPLICIT_NONCE_LEN);
                payload.truncate(tag_pos);
                Ok(m.into_plain_message())
            }
        }
    };
}

impl_gcm_tls13!(
    Tls13Aes128Gcm,
    Tls13CipherAes128Gcm,
    aes_gcm::Aes128Gcm,
    Aes128Gcm,
    16
);
impl_gcm_tls13!(
    Tls13Aes256Gcm,
    Tls13CipherAes256Gcm,
    aes_gcm::Aes256Gcm,
    Aes256Gcm,
    16
);
impl_gcm_tls12!(
    Tls12Aes128Gcm,
    Tls12CipherAes128GcmEnc,
    Tls12CipherAes128GcmDec,
    aes_gcm::Aes128Gcm,
    Aes128Gcm
);
impl_gcm_tls12!(
    Tls12Aes256Gcm,
    Tls12CipherAes256GcmEnc,
    Tls12CipherAes256GcmDec,
    aes_gcm::Aes256Gcm,
    Aes256Gcm
);
