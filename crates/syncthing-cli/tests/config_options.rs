//! `config set` options: value validation.

use syncthing_cli::config::{Config, ConfigError};

#[test]
fn rescan_interval_must_be_a_positive_number_of_seconds() {
    let mut cfg = Config::default();
    cfg.set_option("rescan-interval", "60").unwrap();
    assert_eq!(cfg.rescan_interval_secs, 60);
    for bad in ["0", "-5", "soon", ""] {
        let err = cfg.set_option("rescan-interval", bad).expect_err(bad);
        assert!(
            matches!(err, ConfigError::InvalidValue { ref key, .. } if key == "rescan-interval"),
            "{bad:?}: {err:?}"
        );
    }
    assert_eq!(
        cfg.rescan_interval_secs, 60,
        "a rejected value changes nothing"
    );
}

#[test]
fn name_must_not_be_blank_and_is_trimmed() {
    let mut cfg = Config::default();
    cfg.set_option("name", "  tablet  ").unwrap();
    assert_eq!(cfg.device_name, "tablet");
    assert!(cfg.set_option("name", "   ").is_err());
    assert_eq!(cfg.device_name, "tablet");
}

#[test]
fn listen_must_be_a_socket_address() {
    let mut cfg = Config::default();
    cfg.set_option("listen", "127.0.0.1:22001").unwrap();
    assert_eq!(cfg.listen, "127.0.0.1:22001");
    assert!(cfg.set_option("listen", "localhost").is_err());
    assert!(cfg.set_option("listen", "127.0.0.1").is_err());
}

#[test]
fn boolean_options_accept_common_spellings_only() {
    let mut cfg = Config::default();
    for (text, expected) in [
        ("on", true),
        ("NO", false),
        ("1", true),
        ("off", false),
        ("Yes", true),
        ("0", false),
    ] {
        cfg.set_option("relays", text).unwrap();
        assert_eq!(cfg.relays, expected, "{text}");
    }
    cfg.set_option("local-discovery", "false").unwrap();
    assert!(!cfg.local_discovery);
    cfg.set_option("global-discovery", "true").unwrap();
    assert!(cfg.global_discovery);
    assert!(cfg.set_option("relays", "maybe").is_err());
}

#[test]
fn unknown_option_names_are_reported() {
    let mut cfg = Config::default();
    match cfg.set_option("colour", "blue") {
        Err(ConfigError::UnknownKey(key)) => assert_eq!(key, "colour"),
        other => panic!("unexpected {other:?}"),
    }
}
