//! PS Vita path quirks: the `device:relative` to `device:/relative` spelling and the choice of
//! the spelling that can actually be listed.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use syncthing_core::storage::vita_fs::with_device_root_slash;
use syncthing_core::FsSyncStorage;

#[test]
fn test_device_root_spelling_requires_an_alphanumeric_device_and_a_relative_path() {
    let spell = |s: &str| with_device_root_slash(Path::new(s));
    assert_eq!(spell("ux0:music"), Some(PathBuf::from("ux0:/music")));
    assert_eq!(spell("ux0:a/b"), Some(PathBuf::from("ux0:/a/b")));
    assert_eq!(spell(":music"), None, "empty device");
    assert_eq!(spell("a/b:music"), None, "device with a separator");
    assert_eq!(spell("ux-0:music"), None, "device with a dash");
    assert_eq!(spell("ux0:"), None, "no relative path");
    assert_eq!(spell("ux0:/music"), None, "already absolute");
    assert_eq!(spell("plain"), None, "no device at all");
}

/// A `device:name` root that cannot be listed falls back to `device:/name` when that one can;
/// otherwise the path is kept unchanged.
#[test]
fn test_accessible_root_prefers_the_spelling_that_can_be_listed() {
    let base = std::env::current_dir().unwrap();
    let unique = format!("vitapath{}", std::process::id());
    let _ = fs::remove_dir_all(base.join(format!("{unique}alt:")));

    // Neither spelling exists: the path is kept as given.
    let missing = format!("{unique}missing:tree");
    let mut storage = FsSyncStorage::new(common::fresh_temp_dir("vita_path_missing"));
    storage.set_folder_path("f", &missing);
    assert_eq!(storage.folder_root("f"), PathBuf::from(&missing));

    // Only the slash spelling can be listed: it is used.
    let alt_dir = base.join(format!("{unique}alt:")).join("tree");
    fs::create_dir_all(&alt_dir).unwrap();
    let plain = format!("{unique}alt:tree");
    storage.set_folder_path("g", &plain);
    let _ = fs::remove_dir_all(base.join(format!("{unique}alt:")));
    assert_eq!(
        storage.folder_root("g"),
        PathBuf::from(format!("{unique}alt:/tree"))
    );
}
