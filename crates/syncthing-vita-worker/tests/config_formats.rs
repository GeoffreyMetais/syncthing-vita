//! The worker reads the app's configuration exactly as the app does: files written by the
//! app's own `ConfigRepository`, literal fixtures of the documented formats (AGENTS.md §5),
//! hostile peer-supplied text, and the identity seed.

mod common;

use common::{data_dir_with_identity, device, fresh_temp_dir, ignored, repo, share, VITA_SEED};
use syncthing_core::{DeviceId, DeviceIdentity, FolderType};
use syncthing_vita_worker::config::fields::{escape_field as worker_escape, unescape_field};
use syncthing_vita_worker::config::records::{parse_devices, parse_ignored, parse_shares};
use syncthing_vita_worker::config::{is_protected_share_path, WorkerConfig};
use vita_syncthing_app::domain::ShareMode;
use vita_syncthing_app::infrastructure::config_fields::escape_field;

fn id(seed: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[seed; 32]).unwrap().device_id
}

#[test]
fn test_reads_what_the_app_writes() {
    let data = data_dir_with_identity("formats_app_roundtrip", true);
    let (a, b) = (id(1), id(2));
    let app = repo(&data);
    let hostile = "Evil|name\nx|y|ux0:|1|%7C";
    app.save_devices(&[
        device(a, hostile, "192.168.1.20:22000"),
        device(b, "Laptop 100%", "dynamic"),
    ]);
    let target = data.join("music");
    let mut ro = share("ro|folder\n", &target, false, vec![a, b]);
    ro.mode = ShareMode::ReadOnly;
    ro.label = "Label|with\r\nbreaks %0A".to_owned();
    app.save_shares(&[share("saves", &data.join("saves"), true, vec![a]), ro]);
    app.save_ignored_folders(&[ignored("declined|x", b)]);

    let cfg = WorkerConfig::load(&data).unwrap();
    assert_eq!(
        cfg.identity.device_id,
        DeviceIdentity::from_seed(&VITA_SEED).unwrap().device_id
    );
    let app_devices = app.load_devices();
    assert_eq!(cfg.devices.len(), 2);
    for (ours, theirs) in cfg.devices.iter().zip(&app_devices) {
        assert_eq!(ours.device_id, theirs.device_id);
        assert_eq!(ours.name, theirs.name);
        assert_eq!(ours.address, theirs.address);
    }
    assert_eq!(cfg.devices[0].name, hostile);

    let app_shares = app.load_shares();
    assert_eq!(cfg.shares.len(), 2);
    for (ours, theirs) in cfg.shares.iter().zip(&app_shares) {
        assert_eq!(ours.folder_id, theirs.folder_id);
        assert_eq!(ours.label, theirs.label);
        assert_eq!(ours.target_path, theirs.target_path);
        assert_eq!(ours.enabled, theirs.enabled);
        assert_eq!(ours.devices, theirs.shared_devices);
        assert_eq!(ours.read_only, theirs.mode == ShareMode::ReadOnly);
    }
    assert_eq!(cfg.shares[1].folder_id, "ro|folder\n");
    assert_eq!(cfg.shares[1].folder_type(), FolderType::ReceiveOnly);
    assert_eq!(cfg.shares[0].folder_type(), FolderType::SendReceive);
    assert!(!cfg.shares[1].enabled);

    assert_eq!(cfg.ignored.len(), 1);
    assert_eq!(cfg.ignored[0].folder_id, "declined|x");
    assert_eq!(cfg.ignored[0].device_id, b);
    assert_eq!(cfg.sync_root(), data.join("sync"));
}

#[test]
fn test_literal_fixtures() {
    let (a, b) = (id(3), id(4));
    let devices = parse_devices(&format!(
        "{}|Desk%7Ctop|10.0.0.2:22000\n\
         not-hex|Bad|1.2.3.4:22000\n\
         {}|Short\n\
         {}|Laptop|dynamic|extra\n",
        a.to_hex(),
        a.to_hex(),
        b.to_hex()
    ));
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].device_id, a);
    assert_eq!(devices[0].name, "Desk|top");
    assert_eq!(devices[0].address, "10.0.0.2:22000");
    assert_eq!(devices[1].device_id, b);
    assert_eq!(devices[1].address, "dynamic");

    let shares = parse_shares(&format!(
        "legacy|Legacy|ux0:data/x|1\n\
         paused|Paused|ux0:music|0|{a},junk , {b}|ro\n\
         empty|Empty|ux0:picture|1||rw\n\
         other|Other|ux0:video|2|{a}|xx\n\
         short|Short|ux0:a\n",
        a = a.to_hex(),
        b = b.to_hex()
    ));
    assert_eq!(shares.len(), 4);
    assert_eq!(shares[0].folder_id, "legacy");
    assert!(shares[0].enabled && !shares[0].read_only && shares[0].devices.is_empty());
    assert_eq!(shares[1].devices, vec![a, b], "invalid entries are dropped");
    assert!(!shares[1].enabled && shares[1].read_only);
    assert_eq!(shares[1].target_path, "ux0:music");
    assert!(shares[2].devices.is_empty() && !shares[2].read_only);
    assert!(shares[3].enabled, "anything but 0 is enabled");
    assert!(!shares[3].read_only, "unknown modes are read & write");
    assert!(shares[1].is_linked_to(&b));
    assert!(!shares[0].is_linked_to(&a));

    let ignored = parse_ignored(&format!(
        "f%7C1|{}\nno-separator\nf2|nothex\nf3| {} \n",
        a.to_hex(),
        b.to_hex()
    ));
    assert_eq!(ignored.len(), 2);
    assert_eq!(ignored[0].folder_id, "f|1");
    assert_eq!(ignored[1].device_id, b);
}

#[test]
fn test_unescape_matches_the_app_encoder() {
    for text in [
        "",
        "plain",
        "a|b",
        "100%",
        "%7C literal",
        "line\nbreak\r\n",
        "%%|%0A%0d",
        "ünïcødé|名前",
    ] {
        assert_eq!(unescape_field(&escape_field(text)), text, "{text:?}");
        assert_eq!(worker_escape(text), escape_field(text), "{text:?}");
    }
    assert_eq!(unescape_field("a%7cb%0a%0D%25"), "a|b\n\r%");
    assert_eq!(unescape_field("50% off %ZZ %"), "50% off %ZZ %");
    assert_eq!(unescape_field("%2"), "%2");
}

#[test]
fn test_missing_or_bad_seed_is_an_error() {
    let dir = fresh_temp_dir("formats_no_seed");
    assert!(WorkerConfig::load(&dir).is_err());
    std::fs::write(dir.join("device_seed.bin"), [1u8; 31]).unwrap();
    assert!(WorkerConfig::load(&dir).is_err());
    std::fs::write(dir.join("device_seed.bin"), [1u8; 33]).unwrap();
    assert!(WorkerConfig::load(&dir).is_err());
    std::fs::write(dir.join("device_seed.bin"), [1u8; 32]).unwrap();
    let cfg = WorkerConfig::load(&dir).unwrap();
    assert!(cfg.devices.is_empty() && cfg.shares.is_empty() && cfg.ignored.is_empty());
}

#[test]
fn test_interrupted_replacement_is_recovered_from_the_backup() {
    let data = data_dir_with_identity("formats_recovery", true);
    let a = id(5);
    repo(&data).save_devices(&[device(a, "Desktop", "1.2.3.4:22000")]);
    // A replacement cut by a power loss: only the previous version remains, as `.bak`.
    std::fs::rename(data.join("devices.cfg"), data.join("devices.cfg.bak")).unwrap();
    let cfg = WorkerConfig::load(&data).unwrap();
    assert_eq!(cfg.devices.len(), 1);
    assert_eq!(cfg.devices[0].name, "Desktop");
}

#[test]
fn test_protected_paths_match_the_app() {
    let data = std::path::Path::new("ux0:data/syncthing");
    let app = vita_syncthing_app::infrastructure::ConfigRepository::new("ux0:data/syncthing");
    for path in [
        "ux0:",
        "ux0:/",
        "UX0:data",
        "ux0:/data/",
        "ux0:data/syncthing",
        "ux0:data/SyncThing/",
        "ux0:data/syncthing/sync",
        "ux0:data/syncthing/shared",
        "ux0:data/other",
        "ux0:music",
        "ur0:",
        "",
    ] {
        assert_eq!(
            is_protected_share_path(data, path),
            app.is_protected_share_path(path),
            "{path:?}"
        );
    }
    assert!(is_protected_share_path(data, "ux0:data"));
    assert!(!is_protected_share_path(data, "ux0:data/syncthing/sync"));
}
