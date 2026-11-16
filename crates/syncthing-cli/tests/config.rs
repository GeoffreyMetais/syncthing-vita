//! Configuration model: defaults, persistence, device / folder / option edits.

mod common;

use std::path::PathBuf;

use syncthing_cli::config::{load_config, save_config, Config, ConfigError};
use syncthing_core::DeviceId;

use common::{temp_home, valid_device_id};

fn device_id(seed: u8) -> DeviceId {
    DeviceId::parse_user_id(&valid_device_id(seed)).expect("valid id")
}

#[test]
fn defaults_are_applied() {
    let cfg = Config::default();
    assert_eq!(cfg.device_name, "syncthing-cli");
    assert_eq!(cfg.listen, "0.0.0.0:22000");
    assert!(cfg.local_discovery);
    assert!(cfg.global_discovery);
    assert!(cfg.relays);
    assert!(cfg.devices.is_empty());
    assert!(cfg.folders.is_empty());
}

#[test]
fn missing_config_file_loads_defaults() {
    let home = temp_home("config-missing");
    assert_eq!(load_config(&home).unwrap(), Config::default());
}

#[test]
fn save_then_load_roundtrip() {
    let home = temp_home("config-roundtrip");
    let mut cfg = Config::default();
    let id = valid_device_id(3);
    cfg.add_device(&id, Some("laptop".into()), vec!["192.168.1.5:22000".into()])
        .unwrap();
    cfg.add_folder(
        "docs",
        PathBuf::from("/tmp/docs"),
        Some("Documents".into()),
        vec![id.clone()],
        true,
    )
    .unwrap();
    cfg.set_folder_paused("docs", true).unwrap();
    cfg.set_option("listen", "0.0.0.0:22001").unwrap();

    save_config(&home, &cfg).unwrap();
    assert_eq!(load_config(&home).unwrap(), cfg);
}

#[test]
fn missing_fields_in_file_take_defaults() {
    let home = temp_home("config-partial");
    std::fs::write(home.join("config.toml"), "device_name = \"tablet\"\n").unwrap();
    let cfg = load_config(&home).unwrap();
    assert_eq!(cfg.device_name, "tablet");
    assert_eq!(cfg.listen, "0.0.0.0:22000");
    assert!(cfg.relays);
    assert!(cfg.folders.is_empty());
}

#[test]
fn malformed_file_is_invalid_data() {
    let home = temp_home("config-malformed");
    std::fs::write(home.join("config.toml"), "devices = [[[\n").unwrap();
    let err = load_config(&home).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn add_device_canonicalizes_and_defaults() {
    let mut cfg = Config::default();
    let raw = valid_device_id(4).to_lowercase().replace('-', " ");
    let added = cfg.add_device(&raw, None, vec![]).unwrap();
    assert_eq!(added.id, valid_device_id(4));
    assert_eq!(added.name, device_id(4).short());
    assert_eq!(added.addresses, vec!["dynamic".to_owned()]);
    assert_eq!(cfg.device(&device_id(4)), Some(&added));
}

#[test]
fn add_device_rejects_invalid_and_short_ids() {
    let mut cfg = Config::default();
    assert!(matches!(
        cfg.add_device("NOT-A-DEVICE", None, vec![]),
        Err(ConfigError::InvalidDeviceId(_))
    ));
    assert!(matches!(
        cfg.add_device("MFZWI3D", None, vec![]),
        Err(ConfigError::InvalidDeviceId(_))
    ));
    assert!(cfg.devices.is_empty());
}

#[test]
fn add_device_rejects_duplicates() {
    let mut cfg = Config::default();
    cfg.add_device(&valid_device_id(5), None, vec![]).unwrap();
    let again = cfg.add_device(&valid_device_id(5).to_lowercase(), None, vec![]);
    assert!(matches!(again, Err(ConfigError::DeviceExists(_))));
    assert_eq!(cfg.devices.len(), 1);
}

#[test]
fn addresses_are_normalized() {
    let mut cfg = Config::default();
    let added = cfg
        .add_device(
            &valid_device_id(6),
            None,
            vec![
                "10.0.0.2:22000".into(),
                "tcp://relay.example:22067".into(),
                "dynamic".into(),
            ],
        )
        .unwrap();
    assert_eq!(
        added.addresses,
        vec![
            "tcp://10.0.0.2:22000".to_owned(),
            "tcp://relay.example:22067".to_owned(),
            "dynamic".to_owned(),
        ]
    );
}

#[test]
fn remove_device_cascades_to_folders() {
    let mut cfg = Config::default();
    let a = valid_device_id(7);
    let b = valid_device_id(8);
    cfg.add_device(&a, None, vec![]).unwrap();
    cfg.add_device(&b, None, vec![]).unwrap();
    cfg.add_folder(
        "f",
        PathBuf::from("/tmp/f"),
        None,
        vec![a.clone(), b.clone()],
        false,
    )
    .unwrap();

    cfg.remove_device(&a).unwrap();
    assert_eq!(cfg.folders[0].devices, vec![b]);
    assert!(cfg.device(&device_id(7)).is_none());
}

#[test]
fn remove_unknown_device_fails() {
    let mut cfg = Config::default();
    assert!(matches!(
        cfg.remove_device(&valid_device_id(9)),
        Err(ConfigError::UnknownDevice(_))
    ));
}

#[test]
fn add_folder_requires_configured_devices() {
    let mut cfg = Config::default();
    let err = cfg
        .add_folder(
            "f",
            PathBuf::from("/tmp/f"),
            None,
            vec![valid_device_id(10)],
            false,
        )
        .unwrap_err();
    assert!(matches!(err, ConfigError::UnknownDevice(_)));
    assert!(cfg.folders.is_empty());
}

#[test]
fn add_folder_defaults_label_and_rejects_duplicates() {
    let mut cfg = Config::default();
    cfg.add_folder("photos", PathBuf::from("/tmp/p"), None, vec![], false)
        .unwrap();
    assert_eq!(cfg.folders[0].label, "photos");
    assert!(!cfg.folders[0].paused);
    let again = cfg.add_folder("photos", PathBuf::from("/tmp/q"), None, vec![], false);
    assert!(matches!(again, Err(ConfigError::FolderExists(_))));
}

#[test]
fn share_is_idempotent_and_unshare_removes() {
    let mut cfg = Config::default();
    let id = valid_device_id(11);
    cfg.add_device(&id, None, vec![]).unwrap();
    cfg.add_folder("f", PathBuf::from("/tmp/f"), None, vec![], false)
        .unwrap();

    cfg.share_folder("f", &id).unwrap();
    cfg.share_folder("f", &id.to_lowercase()).unwrap();
    assert_eq!(cfg.folders[0].devices, vec![id.clone()]);

    cfg.unshare_folder("f", &id).unwrap();
    assert!(cfg.folders[0].devices.is_empty());
    cfg.unshare_folder("f", &id).unwrap();
}

#[test]
fn share_requires_known_folder_and_device() {
    let mut cfg = Config::default();
    let id = valid_device_id(12);
    cfg.add_device(&id, None, vec![]).unwrap();
    assert!(matches!(
        cfg.share_folder("missing", &id),
        Err(ConfigError::UnknownFolder(_))
    ));
    cfg.add_folder("f", PathBuf::from("/tmp/f"), None, vec![], false)
        .unwrap();
    assert!(matches!(
        cfg.share_folder("f", &valid_device_id(13)),
        Err(ConfigError::UnknownDevice(_))
    ));
}

#[test]
fn pause_and_resume_folder() {
    let mut cfg = Config::default();
    cfg.add_folder("f", PathBuf::from("/tmp/f"), None, vec![], false)
        .unwrap();
    cfg.set_folder_paused("f", true).unwrap();
    assert!(cfg.folders[0].paused);
    cfg.set_folder_paused("f", false).unwrap();
    assert!(!cfg.folders[0].paused);
    assert!(matches!(
        cfg.set_folder_paused("nope", true),
        Err(ConfigError::UnknownFolder(_))
    ));
}

#[test]
fn remove_folder_removes_entry() {
    let mut cfg = Config::default();
    cfg.add_folder("f", PathBuf::from("/tmp/f"), None, vec![], false)
        .unwrap();
    cfg.remove_folder("f").unwrap();
    assert!(cfg.folders.is_empty());
    assert!(matches!(
        cfg.remove_folder("f"),
        Err(ConfigError::UnknownFolder(_))
    ));
}

#[test]
fn set_option_accepts_valid_values() {
    let mut cfg = Config::default();
    cfg.set_option("name", "  studio  ").unwrap();
    cfg.set_option("listen", "127.0.0.1:22001").unwrap();
    cfg.set_option("local-discovery", "off").unwrap();
    cfg.set_option("global-discovery", "No").unwrap();
    cfg.set_option("relays", "1").unwrap();
    assert_eq!(cfg.device_name, "studio");
    assert_eq!(cfg.listen, "127.0.0.1:22001");
    assert!(!cfg.local_discovery);
    assert!(!cfg.global_discovery);
    assert!(cfg.relays);
}

#[test]
fn set_option_rejects_invalid_values_and_unknown_keys() {
    let mut cfg = Config::default();
    assert!(matches!(
        cfg.set_option("listen", "not-an-address"),
        Err(ConfigError::InvalidValue { .. })
    ));
    assert!(matches!(
        cfg.set_option("relays", "maybe"),
        Err(ConfigError::InvalidValue { .. })
    ));
    assert!(matches!(
        cfg.set_option("name", "   "),
        Err(ConfigError::InvalidValue { .. })
    ));
    assert!(matches!(
        cfg.set_option("bogus", "1"),
        Err(ConfigError::UnknownKey(_))
    ));
    assert_eq!(cfg, Config::default());
}
