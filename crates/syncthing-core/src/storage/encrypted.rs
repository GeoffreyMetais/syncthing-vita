//! Pure-Rust cryptographic primitives and policy helpers for `FolderType::ReceiveEncrypted`
//! (Syncthing BEP folder encryption, BEP spec "Untrusted (Encrypted) Devices").
//!
//! Uses only workspace crates already in `syncthing-core` (`sha2`, `hmac`, `chacha20poly1305`)
//! with zero new external dependencies:
//! - Key derivation (`derive_folder_key`): HKDF-SHA256 (RFC 5869) over `folder_id` + password.
//! - Block encryption / decryption (`encrypt_block`, `decrypt_block`): XChaCha20-Poly1305 with
//!   random/caller-supplied 24-byte nonce (`nonce || ciphertext || 16-byte tag`).
//! - Filename token (`encrypt_filename_token`): deterministic HKDF-SHA256 + Base32-style hex tag
//!   for encrypted `.syncthing-enc` paths.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, KeyInit as _, Mac};
use sha2::{Digest, Sha256};
use std::io;

use crate::bep::FolderType;

type HmacSha256 = Hmac<Sha256>;

/// Length in bytes of a derived Syncthing folder encryption key (256 bits).
pub const FOLDER_KEY_LEN: usize = 32;
/// Nonce length in bytes for XChaCha20-Poly1305 block encryption (192 bits).
pub const BLOCK_NONCE_LEN: usize = 24;
/// Authentication tag length in bytes for Poly1305 (128 bits).
pub const BLOCK_TAG_LEN: usize = 16;

const HKDF_INFO_KEY: &[u8] = b"syncthing-vita-folder-encryption-key-v1";
const HKDF_INFO_NAME: &[u8] = b"syncthing-vita-filename-key-v1";

/// Returns `true` when `folder_type` must never send local plaintext changes back to untrusted
/// peers (both [`FolderType::ReceiveOnly`] and [`FolderType::ReceiveEncrypted`]).
pub const fn suppresses_local_outbound_changes(folder_type: FolderType) -> bool {
    matches!(
        folder_type,
        FolderType::ReceiveOnly | FolderType::ReceiveEncrypted
    )
}

/// Derives a 32-byte folder encryption key from `(folder_id, password)` using HKDF-SHA256
/// (RFC 5869 extract + expand, salt = `SHA-256("syncthing" || folder_id)`).
pub fn derive_folder_key(folder_id: &str, password: &str) -> [u8; FOLDER_KEY_LEN] {
    let mut salt_hasher = Sha256::new();
    salt_hasher.update(b"syncthing");
    salt_hasher.update(folder_id.as_bytes());
    let salt = salt_hasher.finalize();

    let prk = hkdf_extract(&salt, password.as_bytes());
    hkdf_expand_32(&prk, HKDF_INFO_KEY)
}

/// Derives a deterministic 32-byte subkey for filename / metadata authentication from `folder_key`.
pub fn derive_metadata_key(folder_key: &[u8; FOLDER_KEY_LEN]) -> [u8; FOLDER_KEY_LEN] {
    let prk = hkdf_extract(folder_key, HKDF_INFO_NAME);
    hkdf_expand_32(&prk, HKDF_INFO_NAME)
}

/// Encrypts a plaintext block with XChaCha20-Poly1305 under `folder_key` and `nonce`, returning
/// `nonce (24 B) || ciphertext || tag (16 B)`.
pub fn encrypt_block(
    folder_key: &[u8; FOLDER_KEY_LEN],
    nonce: &[u8; BLOCK_NONCE_LEN],
    plaintext: &[u8],
) -> io::Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(folder_key.into());
    let xnonce = XNonce::from_slice(nonce);
    let ciphertext = cipher
        .encrypt(xnonce, plaintext)
        .map_err(|_| io::Error::other("XChaCha20-Poly1305 encryption failed"))?;
    let mut out = Vec::with_capacity(BLOCK_NONCE_LEN + ciphertext.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts a block produced by [`encrypt_block`] (`nonce (24 B) || ciphertext || tag (16 B)`).
pub fn decrypt_block(folder_key: &[u8; FOLDER_KEY_LEN], sealed: &[u8]) -> io::Result<Vec<u8>> {
    if sealed.len() < BLOCK_NONCE_LEN + BLOCK_TAG_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "encrypted block too short for nonce and authentication tag",
        ));
    }
    let (nonce_bytes, payload) = sealed.split_at(BLOCK_NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(folder_key.into());
    let xnonce = XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(xnonce, payload)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "block authentication failed"))
}

/// Computes a deterministic encrypted relative path token for `rel_path` under `folder_key`
/// using HMAC-SHA256 (Syncthing-style `.syncthing-enc/<prefix>/<token>` hierarchy).
pub fn encrypted_rel_path(folder_key: &[u8; FOLDER_KEY_LEN], rel_path: &str) -> String {
    let meta_key = derive_metadata_key(folder_key);
    let mut mac = HmacSha256::new_from_slice(&meta_key).expect("HMAC accepts any key length");
    mac.update(rel_path.as_bytes());
    let tag = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(tag.len() * 2);
    for b in tag {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{b:02X}");
    }
    format!("{}.syncthing-enc/{}/{}", &hex[0..1], &hex[1..3], &hex[3..])
}

fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(salt).expect("HMAC accepts any key length");
    mac.update(ikm);
    let out = mac.finalize().into_bytes();
    let mut prk = [0u8; 32];
    prk.copy_from_slice(&out);
    prk
}

fn hkdf_expand_32(prk: &[u8; 32], info: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(prk).expect("HMAC accepts any key length");
    mac.update(info);
    mac.update(&[1u8]);
    let out = mac.finalize().into_bytes();
    let mut okm = [0u8; 32];
    okm.copy_from_slice(&out);
    okm
}
