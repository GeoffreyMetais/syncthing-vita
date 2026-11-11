//! Deterministic ECDSA P-256 X.509 identity generation.

use syncthing_core::VitaIdentity;

#[test]
fn test_vita_x509_identity_generation() {
    let seed = *b"VITA_SYNCTHING_ECDSA_SEED_000001";
    let id1 = VitaIdentity::from_seed(&seed).expect("X.509 identity generation failed");
    let id2 = VitaIdentity::from_seed(&seed).expect("X.509 identity generation failed");
    assert_eq!(id1.device_id, id2.device_id);
    assert!(!id1.cert_der.is_empty());
    assert!(!id1.key_pkcs8_der.is_empty());
}
