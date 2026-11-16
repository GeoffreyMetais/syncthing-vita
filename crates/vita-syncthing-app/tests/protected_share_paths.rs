//! Share folders that must never cover the app's own data (`ConfigRepository::is_protected_share_path`),
//! the default share directory, and the `ux0:` directory listing used by the `Files` browser.

mod common;

use std::fs;

use common::temp_test_dir;
use vita_syncthing_app::infrastructure::ConfigRepository;

fn repo() -> ConfigRepository {
    ConfigRepository::new("ux0:data/syncthing")
}

#[test]
fn test_app_data_directory_and_its_parents_are_protected() {
    let repo = repo();
    assert!(repo.is_protected_share_path("ux0:"));
    assert!(repo.is_protected_share_path("ux0:data"));
    assert!(repo.is_protected_share_path("ux0:data/syncthing"));
    // Comparison ignores case, slash spelling and `.` components (PS Vita storage is
    // case-insensitive).
    assert!(repo.is_protected_share_path("UX0:/Data/SyncThing/"));
    assert!(repo.is_protected_share_path("ux0:/data//./syncthing"));
}

#[test]
fn test_folders_inside_or_beside_the_data_directory_are_allowed() {
    let repo = repo();
    // Inside the data directory: holds no seed nor configuration.
    assert!(!repo.is_protected_share_path("ux0:data/syncthing/sync/music"));
    assert!(!repo.is_protected_share_path("ux0:data/syncthing/shared"));
    // Unrelated folders, including ones sharing a name prefix with the data directory.
    assert!(!repo.is_protected_share_path("ux0:music"));
    assert!(!repo.is_protected_share_path("ux0:user/00/savedata"));
    assert!(!repo.is_protected_share_path("ux0:data/syncthingx"));
    assert!(!repo.is_protected_share_path("ux0:data/retroarch/savefiles"));
    assert!(!repo.is_protected_share_path("ux1:"));
    assert!(!repo.is_protected_share_path("ux0:data/syncthing/../other"));
}

#[test]
fn test_protection_applies_to_host_style_paths() {
    let repo = ConfigRepository::new("/tmp/syncthing-vita");
    assert!(repo.is_protected_share_path("/tmp"));
    assert!(repo.is_protected_share_path("/tmp/syncthing-vita/"));
    assert!(!repo.is_protected_share_path("/tmp/syncthing-vita/sync/photos"));
    assert!(!repo.is_protected_share_path("/home/user"));
}

#[test]
fn test_default_browse_root_is_the_host_stand_in_for_ux0() {
    assert_eq!(
        ConfigRepository::default_browse_root(),
        "/tmp/syncthing-vita"
    );
}

#[test]
fn test_list_subdirectories_is_sorted_and_hides_dot_entries_and_files() {
    let dir = temp_test_dir("list-subdirs");
    fs::create_dir_all(format!("{dir}/zeta")).unwrap();
    fs::create_dir_all(format!("{dir}/alpha")).unwrap();
    fs::create_dir_all(format!("{dir}/.hidden")).unwrap();
    fs::write(format!("{dir}/file.txt"), b"data").unwrap();

    let repo = repo();
    assert_eq!(repo.list_subdirectories(&dir), vec!["alpha", "zeta"]);
}

#[test]
fn test_list_subdirectories_of_missing_or_empty_directory_is_empty() {
    let dir = temp_test_dir("list-empty");
    let repo = repo();
    assert!(repo
        .list_subdirectories(&format!("{dir}/missing"))
        .is_empty());
    assert!(repo.list_subdirectories(&dir).is_empty());
}

#[test]
fn test_list_subdirectories_keeps_names_with_dots_in_the_middle() {
    let dir = temp_test_dir("list-dots");
    fs::create_dir_all(format!("{dir}/a.b")).unwrap();
    fs::create_dir_all(format!("{dir}/..x")).unwrap();
    let names = repo().list_subdirectories(&dir);
    assert!(names.contains(&"a.b".to_owned()));
    assert!(!names.iter().any(|n| n.starts_with('.')));
}
