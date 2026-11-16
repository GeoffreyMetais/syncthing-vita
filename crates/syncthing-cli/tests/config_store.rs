//! Loading `config.toml`: a missing file means the defaults, any other read error is an error.

use syncthing_cli::config::{load_config, Config};

use common::temp_home;

mod common;

#[test]
fn a_config_file_that_cannot_be_read_is_an_error() {
    let home = temp_home("store-unreadable");
    std::fs::create_dir_all(home.join("config.toml")).unwrap();
    let err = load_config(&home).expect_err("a directory is not a readable file");
    assert_ne!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn a_malformed_config_file_is_refused_with_its_path() {
    let home = temp_home("store-malformed");
    std::fs::write(home.join("config.toml"), "device_name = [unclosed\n").unwrap();
    let err = load_config(&home).expect_err("malformed TOML");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("config.toml"), "{err}");
}

#[test]
fn a_missing_config_file_is_the_default_configuration() {
    let home = temp_home("store-missing");
    assert_eq!(load_config(&home).unwrap(), Config::default());
}
