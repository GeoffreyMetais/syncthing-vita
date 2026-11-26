//! Exact on-disk formats of the `.cfg` files of `ConfigRepository` (AGENTS.md §5): the raw text
//! written by each `save_*` is checked byte for byte, and each `load_*` is checked against
//! legacy, malformed and defaulted lines.

mod common;

use std::fs;

use common::temp_test_dir;
use syncthing_core::DeviceId;
use vita_syncthing_app::domain::{
    IgnoredFolder, KnownDevice, Language, LanguagePreference, ShareConfigEntry, ShareMode,
    ThemeMode,
};
use vita_syncthing_app::infrastructure::ConfigRepository;

const DEV_A: DeviceId = DeviceId([0xab; 32]);
const DEV_B: DeviceId = DeviceId([0x01; 32]);

fn read(dir: &str, name: &str) -> String {
    fs::read_to_string(format!("{dir}/{name}")).unwrap()
}

fn write(dir: &str, name: &str, content: &str) {
    fs::write(format!("{dir}/{name}"), content).unwrap();
}

fn share(folder_id: &str, label: &str, target: &str, enabled: bool) -> ShareConfigEntry {
    ShareConfigEntry {
        folder_id: folder_id.to_owned(),
        label: label.to_owned(),
        target_path: target.to_owned(),
        enabled,
        mode: ShareMode::ReadWrite,
        shared_devices: Vec::new(),
    }
}

#[test]
fn test_devices_cfg_exact_format_and_escaping() {
    let dir = temp_test_dir("cfg-devices-format");
    let repo = ConfigRepository::new(&dir);
    repo.save_devices(&[
        KnownDevice {
            device_id: DEV_A,
            name: "Desk|top".to_owned(),
            address: "192.168.1.10:22000".to_owned(),
        },
        KnownDevice {
            device_id: DEV_B,
            name: "dynamic".to_owned(),
            address: "dynamic".to_owned(),
        },
    ]);
    let expected = format!(
        "{}|Desk%7Ctop|192.168.1.10:22000\n{}|dynamic|dynamic\n",
        DEV_A.to_hex(),
        DEV_B.to_hex()
    );
    assert_eq!(read(&dir, "devices.cfg"), expected);

    let loaded = repo.load_devices();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].device_id, DEV_A);
    assert_eq!(loaded[0].name, "Desk|top");
    assert_eq!(loaded[0].address, "192.168.1.10:22000");
    assert_eq!(loaded[1].device_id, DEV_B);
    assert_eq!(loaded[1].address, "dynamic");
}

#[test]
fn test_devices_cfg_skips_malformed_lines_and_keeps_extra_fields_ignored() {
    let dir = temp_test_dir("cfg-devices-malformed");
    let repo = ConfigRepository::new(&dir);
    write(
        &dir,
        "devices.cfg",
        &format!(
            "not-a-device|Name|1.2.3.4:22000\n\
             {}|OnlyTwo\n\
             {}|Name|1.2.3.4:22000|extra\n\
             \n\
             {}|Old%20name|5.6.7.8:22000\n",
            DEV_A.to_hex(),
            DEV_B.to_hex(),
            DEV_B.to_hex()
        ),
    );
    let loaded = repo.load_devices();
    // Bad hex and lines with fewer than three fields are dropped; extra fields are ignored.
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].device_id, DEV_B);
    assert_eq!(loaded[0].name, "Name");
    assert_eq!(loaded[1].name, "Old%20name");
    assert_eq!(loaded[1].address, "5.6.7.8:22000");
}

#[test]
fn test_devices_cfg_missing_file_loads_empty() {
    let dir = temp_test_dir("cfg-devices-missing");
    assert!(ConfigRepository::new(&dir).load_devices().is_empty());
}

#[test]
fn test_shares_cfg_exact_format_with_devices_and_modes() {
    let dir = temp_test_dir("cfg-shares-format");
    let repo = ConfigRepository::new(&dir);
    let mut linked = share("photos", "Photos|Vita", "ux0:picture", true);
    linked.shared_devices = vec![DEV_A, DEV_B];
    let mut read_only = share("saves", "Saves", "ux0:data/saves", false);
    read_only.mode = ShareMode::ReadOnly;
    repo.save_shares(&[linked, read_only]);

    let expected = format!(
        "photos|Photos%7CVita|ux0:picture|1|{},{}|rw\n\
         saves|Saves|ux0:data/saves|0||ro\n",
        DEV_A.to_hex(),
        DEV_B.to_hex()
    );
    assert_eq!(read(&dir, "shares.cfg"), expected);

    let loaded = repo.load_shares();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].folder_id, "photos");
    assert_eq!(loaded[0].label, "Photos|Vita");
    assert_eq!(loaded[0].target_path, "ux0:picture");
    assert!(loaded[0].enabled);
    assert_eq!(loaded[0].mode, ShareMode::ReadWrite);
    assert_eq!(loaded[0].shared_devices, vec![DEV_A, DEV_B]);
    assert!(!loaded[1].enabled);
    assert_eq!(loaded[1].mode, ShareMode::ReadOnly);
    assert!(loaded[1].shared_devices.is_empty());
}

#[test]
fn test_shares_cfg_legacy_lines_default_to_enabled_read_write() {
    let dir = temp_test_dir("cfg-shares-legacy");
    let repo = ConfigRepository::new(&dir);
    // Four fields: no device list, no mode. Only `0` disables a share.
    write(
        &dir,
        "shares.cfg",
        "a|A|ux0:music|1\nb|B|ux0:pictures|0\nc|C|ux0:x|yes|\n",
    );
    let loaded = repo.load_shares();
    assert_eq!(loaded.len(), 3);
    assert!(loaded[0].enabled);
    assert_eq!(loaded[0].mode, ShareMode::ReadWrite);
    assert!(loaded[0].shared_devices.is_empty());
    assert!(!loaded[1].enabled);
    assert!(loaded[2].enabled, "any value other than 0 means enabled");
    assert_eq!(loaded[2].mode, ShareMode::ReadWrite);
}

#[test]
fn test_shares_cfg_skips_short_lines_and_invalid_device_entries() {
    let dir = temp_test_dir("cfg-shares-invalid");
    let repo = ConfigRepository::new(&dir);
    write(
        &dir,
        "shares.cfg",
        &format!(
            "short|only|three\n\
             ok|Ok|ux0:x|1|{},garbage, {} |ro\n",
            DEV_A.to_hex(),
            DEV_B.to_hex()
        ),
    );
    let loaded = repo.load_shares();
    assert_eq!(
        loaded.len(),
        1,
        "lines with fewer than four fields are dropped"
    );
    assert_eq!(loaded[0].shared_devices, vec![DEV_A, DEV_B]);
    assert_eq!(loaded[0].mode, ShareMode::ReadOnly);
}

#[test]
fn test_shares_cfg_default_template_path_is_migrated_and_saved() {
    let dir = temp_test_dir("cfg-shares-migrate");
    let repo = ConfigRepository::new(&dir);
    // `music` is a preset whose path is `ux0:music`: a share still on the default sync
    // directory is moved to the preset path, and the file is rewritten at once.
    let default_dir = repo.default_share_dir("music");
    write(
        &dir,
        "shares.cfg",
        &format!("music|Music|{default_dir}|1||rw\n"),
    );
    let loaded = repo.load_shares();
    assert_eq!(loaded[0].target_path, "ux0:music");
    assert_eq!(read(&dir, "shares.cfg"), "music|Music|ux0:music|1||rw\n");

    // A non-template folder keeps its default directory.
    let custom = repo.default_share_dir("custom");
    write(
        &dir,
        "shares.cfg",
        &format!("custom|Custom|{custom}|1||rw\n"),
    );
    assert_eq!(repo.load_shares()[0].target_path, custom);
    assert_eq!(
        read(&dir, "shares.cfg"),
        format!("custom|Custom|{custom}|1||rw\n")
    );
}

#[test]
fn test_default_share_dir_keeps_only_safe_characters() {
    let repo = ConfigRepository::new("ux0:data/syncthing");
    assert_eq!(
        repo.default_share_dir("abc-DEF_09"),
        "ux0:data/syncthing/sync/abc-DEF_09"
    );
    assert_eq!(
        repo.default_share_dir("../../etc"),
        "ux0:data/syncthing/sync/etc"
    );
    assert_eq!(
        repo.default_share_dir("a b/c.d"),
        "ux0:data/syncthing/sync/abcd"
    );
    assert_eq!(
        repo.default_share_dir("..."),
        "ux0:data/syncthing/sync/folder"
    );
    assert_eq!(repo.default_share_dir(""), "ux0:data/syncthing/sync/folder");
}

#[test]
fn test_ignored_folders_cfg_exact_format() {
    let dir = temp_test_dir("cfg-ignored-format");
    let repo = ConfigRepository::new(&dir);
    repo.save_ignored_folders(&[
        IgnoredFolder {
            folder_id: "docs|x".to_owned(),
            device_id: DEV_A,
        },
        IgnoredFolder {
            folder_id: "plain".to_owned(),
            device_id: DEV_B,
        },
    ]);
    assert_eq!(
        read(&dir, "ignored_folders.cfg"),
        format!("docs%7Cx|{}\nplain|{}\n", DEV_A.to_hex(), DEV_B.to_hex())
    );
    let loaded = repo.load_ignored_folders();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].folder_id, "docs|x");
    assert_eq!(loaded[0].device_id, DEV_A);
    assert_eq!(loaded[1].folder_id, "plain");
}

#[test]
fn test_ignored_folders_cfg_skips_lines_without_separator_or_valid_device() {
    let dir = temp_test_dir("cfg-ignored-malformed");
    let repo = ConfigRepository::new(&dir);
    write(
        &dir,
        "ignored_folders.cfg",
        &format!("no-separator\nbad|zz\nok|  {}  \n", DEV_A.to_hex()),
    );
    let loaded = repo.load_ignored_folders();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].folder_id, "ok");
    assert_eq!(loaded[0].device_id, DEV_A);
    assert!(ConfigRepository::new(&temp_test_dir("cfg-ignored-none"))
        .load_ignored_folders()
        .is_empty());
}

#[test]
fn test_theme_cfg_exact_format_and_defaults() {
    let dir = temp_test_dir("cfg-theme");
    let repo = ConfigRepository::new(&dir);
    assert_eq!(
        repo.load_theme(),
        ThemeMode::Oled,
        "missing file defaults to OLED"
    );

    repo.save_theme(ThemeMode::Dark);
    assert_eq!(read(&dir, "theme.cfg"), "dark\n");
    assert_eq!(repo.load_theme(), ThemeMode::Dark);

    repo.save_theme(ThemeMode::Oled);
    assert_eq!(read(&dir, "theme.cfg"), "oled\n");
    assert_eq!(repo.load_theme(), ThemeMode::Oled);

    repo.save_theme(ThemeMode::SyncthingDark);
    assert_eq!(read(&dir, "theme.cfg"), "syncthing-dark\n");
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingDark);

    repo.save_theme(ThemeMode::SyncthingLight);
    assert_eq!(read(&dir, "theme.cfg"), "syncthing-light\n");
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingLight);

    write(&dir, "theme.cfg", "  DARK  \n");
    assert_eq!(repo.load_theme(), ThemeMode::Dark);
    write(&dir, "theme.cfg", "0\n");
    assert_eq!(repo.load_theme(), ThemeMode::Dark);
    write(&dir, "theme.cfg", "2\n");
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingDark);
    write(&dir, "theme.cfg", "3\n");
    assert_eq!(repo.load_theme(), ThemeMode::SyncthingLight);
    write(&dir, "theme.cfg", "purple\n");
    assert_eq!(
        repo.load_theme(),
        ThemeMode::Oled,
        "invalid content defaults to OLED"
    );
}

#[test]
fn test_language_cfg_exact_format_and_defaults() {
    let dir = temp_test_dir("cfg-language");
    let repo = ConfigRepository::new(&dir);
    assert_eq!(repo.load_language(), LanguagePreference::System);

    repo.save_language(LanguagePreference::Fixed(Language::German));
    assert_eq!(read(&dir, "language.cfg"), "de\n");
    assert_eq!(
        repo.load_language(),
        LanguagePreference::Fixed(Language::German)
    );

    repo.save_language(LanguagePreference::System);
    assert_eq!(read(&dir, "language.cfg"), "system\n");
    assert_eq!(repo.load_language(), LanguagePreference::System);

    write(&dir, "language.cfg", "  FR \n");
    assert_eq!(
        repo.load_language(),
        LanguagePreference::Fixed(Language::French)
    );
    write(&dir, "language.cfg", "klingon\n");
    assert_eq!(repo.load_language(), LanguagePreference::System);
}

#[test]
fn test_save_creates_missing_data_directory() {
    let root = temp_test_dir("cfg-create-dir");
    let dir = format!("{root}/nested/syncthing");
    let repo = ConfigRepository::new(&dir);
    repo.save_theme(ThemeMode::Dark);
    repo.save_language(LanguagePreference::System);
    repo.save_devices(&[]);
    assert_eq!(read(&dir, "theme.cfg"), "dark\n");
    assert_eq!(read(&dir, "devices.cfg"), "");
    assert_eq!(repo.base_data_dir(), dir);
}
