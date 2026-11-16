//! Device identity persistence.

mod common;

use std::io::ErrorKind;

use syncthing_cli::identity::load_or_create_identity;
use syncthing_cli::paths::SEED_FILE;

use common::temp_home;

#[test]
fn identity_is_created_once_and_stable() {
    let home = temp_home("identity-stable");
    let first = load_or_create_identity(&home).unwrap();
    let second = load_or_create_identity(&home).unwrap();
    assert_eq!(first.device_id, second.device_id);
    assert_eq!(first.cert_der, second.cert_der);
}

#[test]
fn seed_file_is_32_bytes_and_private() {
    let home = temp_home("identity-seed");
    load_or_create_identity(&home).unwrap();
    let path = home.join(SEED_FILE);
    assert_eq!(std::fs::read(&path).unwrap().len(), 32);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn distinct_homes_get_distinct_identities() {
    let a = load_or_create_identity(&temp_home("identity-a")).unwrap();
    let b = load_or_create_identity(&temp_home("identity-b")).unwrap();
    assert_ne!(a.device_id, b.device_id);
}

#[test]
fn corrupt_seed_file_is_an_error() {
    let home = temp_home("identity-corrupt");
    std::fs::write(home.join(SEED_FILE), [1u8; 10]).unwrap();
    let err = load_or_create_identity(&home)
        .err()
        .expect("corrupt seed rejected");
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn a_seed_of_the_wrong_length_is_refused_and_left_alone() {
    let home = temp_home("identity-bad-length");
    let path = home.join(SEED_FILE);
    std::fs::write(&path, [1u8; 31]).unwrap();
    let err = load_or_create_identity(&home).err().expect("31-byte seed");
    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("must be exactly 32 bytes"),
        "{err}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), vec![1u8; 31]);
}

#[test]
fn an_unreadable_seed_is_an_error_and_never_replaced() {
    // A directory where the seed should be: reading it fails, which is not "no seed yet".
    let home = temp_home("identity-unreadable");
    let path = home.join(SEED_FILE);
    std::fs::create_dir_all(&path).unwrap();
    assert!(load_or_create_identity(&home).is_err());
    assert!(path.is_dir(), "the directory is not replaced by a seed");
    assert!(
        !home.join(format!("{SEED_FILE}.tmp")).exists(),
        "no seed is generated on a read error"
    );
}
