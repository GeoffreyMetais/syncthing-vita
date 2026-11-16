//! Device identity persistence (`device_seed.bin` & `cert.der`): the seed is created once,
//! reused on every later start, replaced when corrupted, and always matches the certificate.

mod common;

use std::fs;

use common::temp_test_dir;
use syncthing_core::DeviceIdentity;
use vita_syncthing_app::infrastructure::IdentityStore;

#[test]
fn test_generated_seeds_are_full_length_and_not_repeated() {
    let a = IdentityStore::generate_random_seed().unwrap();
    let b = IdentityStore::generate_random_seed().unwrap();
    assert_eq!(a.len(), 32);
    assert!(a.iter().any(|&byte| byte != 0));
    assert_ne!(a, b, "two fresh seeds must differ");
}

#[test]
fn test_first_start_writes_seed_and_certificate() {
    let root = temp_test_dir("identity-first");
    let dir = format!("{root}/nested/data");
    let identity = IdentityStore::load_or_create(&dir).unwrap();

    let seed = fs::read(format!("{dir}/device_seed.bin")).unwrap();
    assert_eq!(seed.len(), 32, "the seed file is exactly 32 raw bytes");
    let from_disk = DeviceIdentity::from_seed(&seed.clone().try_into().unwrap()).unwrap();
    assert_eq!(identity.device_id, from_disk.device_id);

    let cert = fs::read(format!("{dir}/cert.der")).unwrap();
    assert_eq!(
        cert, identity.cert_der,
        "cert.der is the certificate of the identity"
    );
    assert!(!cert.is_empty());
}

#[test]
fn test_later_starts_reuse_the_stored_seed() {
    let dir = temp_test_dir("identity-reuse");
    let first = IdentityStore::load_or_create(&dir).unwrap();
    let seed_before = fs::read(format!("{dir}/device_seed.bin")).unwrap();

    let second = IdentityStore::load_or_create(&dir).unwrap();
    assert_eq!(first.device_id, second.device_id);
    assert_eq!(first.cert_der, second.cert_der);
    assert_eq!(
        fs::read(format!("{dir}/device_seed.bin")).unwrap(),
        seed_before
    );
}

#[test]
fn test_stored_seed_is_used_verbatim() {
    let dir = temp_test_dir("identity-verbatim");
    let seed = [0x5au8; 32];
    fs::write(format!("{dir}/device_seed.bin"), seed).unwrap();
    let identity = IdentityStore::load_or_create(&dir).unwrap();
    let expected = DeviceIdentity::from_seed(&seed).unwrap();
    assert_eq!(identity.device_id, expected.device_id);
    assert_eq!(fs::read(format!("{dir}/device_seed.bin")).unwrap(), seed);
}

#[test]
fn test_corrupted_seed_file_is_replaced_by_a_fresh_one() {
    let dir = temp_test_dir("identity-corrupt");
    // A truncated seed cannot be used: a new 32-byte seed takes its place.
    fs::write(format!("{dir}/device_seed.bin"), [1u8; 31]).unwrap();
    let identity = IdentityStore::load_or_create(&dir).unwrap();
    let seed = fs::read(format!("{dir}/device_seed.bin")).unwrap();
    assert_eq!(seed.len(), 32);
    assert_eq!(
        identity.device_id,
        DeviceIdentity::from_seed(&seed.try_into().unwrap())
            .unwrap()
            .device_id
    );
}
