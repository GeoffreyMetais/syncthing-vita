//! `.cfg` field escaping: text received from peers (device names, folder IDs and labels) can
//! never forge fields or lines in `devices.cfg`, `shares.cfg` or `ignored_folders.cfg`.

mod common;

use common::temp_test_dir;
use syncthing_core::DeviceId;
use vita_syncthing_app::domain::{IgnoredFolder, KnownDevice, ShareConfigEntry, ShareMode};
use vita_syncthing_app::infrastructure::config_fields::{escape_field, unescape_field};
use vita_syncthing_app::infrastructure::ConfigRepository;

const PEER: DeviceId = DeviceId([0x42; 32]);
const HOSTILE: &str = "a|b\nc\rd%7Ce 100%";

#[test]
fn test_escape_field_roundtrip_has_no_separator() {
    let escaped = escape_field(HOSTILE);
    assert!(!escaped.contains(['|', '\n', '\r']));
    assert_eq!(unescape_field(&escaped), HOSTILE);
}

#[test]
fn test_unescape_keeps_legacy_percent_text() {
    assert_eq!(unescape_field("ux0:music/100% hits"), "ux0:music/100% hits");
    assert_eq!(unescape_field("50%"), "50%");
    assert_eq!(unescape_field("%zz%7c"), "%zz|");
}

#[test]
fn test_hostile_fields_roundtrip_through_config_files() {
    let dir = temp_test_dir("cfg-hostile");
    let repo = ConfigRepository::new(&dir);

    repo.save_devices(&[KnownDevice {
        device_id: PEER,
        name: HOSTILE.to_owned(),
        address: "192.168.1.10:22000".to_owned(),
    }]);
    let devices = repo.load_devices();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, HOSTILE);

    repo.save_shares(&[ShareConfigEntry {
        folder_id: HOSTILE.to_owned(),
        label: HOSTILE.to_owned(),
        target_path: "ux0:music/100% hits".to_owned(),
        enabled: true,
        mode: ShareMode::ReadWrite,
        shared_devices: vec![PEER.to_hex()],
    }]);
    let shares = repo.load_shares();
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].folder_id, HOSTILE);
    assert_eq!(shares[0].label, HOSTILE);
    assert_eq!(shares[0].target_path, "ux0:music/100% hits");
    assert_eq!(shares[0].shared_devices, vec![PEER.to_hex()]);

    repo.save_ignored_folders(&[IgnoredFolder {
        folder_id: HOSTILE.to_owned(),
        device_id: PEER,
    }]);
    let ignored = repo.load_ignored_folders();
    assert_eq!(ignored.len(), 1);
    assert_eq!(ignored[0].folder_id, HOSTILE);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_invalid_linked_device_entries_are_dropped_on_load() {
    let dir = temp_test_dir("cfg-bad-devices");
    let repo = ConfigRepository::new(&dir);
    std::fs::write(
        format!("{}/shares.cfg", dir),
        format!("docs|Docs|ux0:docs|1|{},not-a-device\n", PEER.to_hex()),
    )
    .unwrap();
    assert_eq!(repo.load_shares()[0].shared_devices, vec![PEER.to_hex()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_interrupted_config_replacement_restores_the_backup() {
    let dir = temp_test_dir("cfg-interrupted");
    let repo = ConfigRepository::new(&dir);
    // State left by a replacement interrupted after the old file was set aside.
    std::fs::write(
        format!("{}/shares.cfg.bak", dir),
        format!("docs|Docs|ux0:docs|1|{}\n", PEER.to_hex()),
    )
    .unwrap();
    std::fs::write(format!("{}/shares.cfg.tmp", dir), "").unwrap();

    let shares = repo.load_shares();
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].folder_id, "docs");

    // A normal save leaves no backup behind.
    repo.save_shares(&shares);
    assert!(!std::path::Path::new(&format!("{}/shares.cfg.bak", dir)).exists());
    assert_eq!(repo.load_shares().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
