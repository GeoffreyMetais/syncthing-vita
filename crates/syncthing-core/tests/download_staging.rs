//! Crash-safe download staging: temp files seeded from local content, streamed block writes,
//! commits, and recovery of interrupted commits found by a scan.

mod common;

use std::fs;

use syncthing_core::{DeviceId, FsSyncStorage};

const OWN: DeviceId = DeviceId([0x21; 32]);

fn names(storage: &FsSyncStorage, folder: &str) -> Vec<String> {
    let (index, _) = storage.build_local_index(folder, OWN, &[]).unwrap();
    index
        .files
        .iter()
        .filter(|f| !f.deleted)
        .map(|f| f.name.clone())
        .collect()
}

#[test]
fn test_prepare_download_seeds_the_temp_file_with_local_content() {
    let dir = common::fresh_temp_dir("staging_seed");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.bin"), b"hello").unwrap();

    storage.prepare_download("f", "a.bin").unwrap();
    let tmp = storage.download_temp_path("f", "a.bin");
    assert_eq!(fs::read(&tmp).unwrap(), b"hello");

    // Without reuse the temp file starts empty, even though the local file exists.
    storage.prepare_download_seeded("f", "a.bin", false).unwrap();
    assert!(tmp.is_file());
    assert_eq!(fs::read(&tmp).unwrap().len(), 0);

    // A new file has no local content to reuse: the temp file is created empty.
    storage.prepare_download("f", "new.bin").unwrap();
    let new_tmp = storage.download_temp_path("f", "new.bin");
    assert!(new_tmp.is_file());
    assert_eq!(fs::read(&new_tmp).unwrap().len(), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_temp_and_backup_names_sit_next_to_the_file() {
    let dir = common::fresh_temp_dir("staging_names");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    assert_eq!(
        storage.download_temp_path("f", "sub/a.bin"),
        root.join("sub").join(".syncthing.a.bin.tmp")
    );
    assert_eq!(
        storage.download_backup_path("f", "sub/a.bin"),
        root.join("sub").join(".syncthing.a.bin.bak")
    );
}

/// The temp file is cut to the final size when it is longer (seeded from a bigger old file),
/// and is not extended when it is shorter.
#[test]
fn test_streamed_write_cuts_the_temp_file_to_the_final_size() {
    let dir = common::fresh_temp_dir("staging_truncate");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("big.bin"), vec![b'o'; 100]).unwrap();

    storage.prepare_download("f", "big.bin").unwrap();
    let tmp = storage
        .write_block_streaming("f", "big.bin", 0, b"xyz", 10)
        .unwrap();
    assert_eq!(tmp, storage.download_temp_path("f", "big.bin"));
    let content = fs::read(&tmp).unwrap();
    assert_eq!(content.len(), 10, "longer temp file is cut to the final size");
    assert_eq!(&content[..3], b"xyz");

    // A shorter temp file is never extended by a block write.
    storage.prepare_download_seeded("f", "small.bin", false).unwrap();
    storage
        .write_block_streaming("f", "small.bin", 0, b"ab", 10)
        .unwrap();
    let tmp = storage.download_temp_path("f", "small.bin");
    assert_eq!(fs::read(&tmp).unwrap(), b"ab");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_commit_moves_the_temp_file_into_place_with_its_final_size() {
    let dir = common::fresh_temp_dir("staging_commit");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("c.bin"), b"previous version").unwrap();

    storage.prepare_download_seeded("f", "c.bin", false).unwrap();
    storage
        .write_block_streaming("f", "c.bin", 0, b"fresh!", 6)
        .unwrap();
    storage.commit_download("f", "c.bin", 6).unwrap();
    assert_eq!(fs::read(root.join("c.bin")).unwrap(), b"fresh!");
    assert!(!storage.download_temp_path("f", "c.bin").exists());
    assert!(!storage.download_backup_path("f", "c.bin").exists());
    let _ = fs::remove_dir_all(&dir);
}

/// A commit interrupted after the old file was set aside is finished by the next scan, which
/// must see the restored file in the same pass (not record it as deleted).
#[test]
fn test_scan_sees_a_file_restored_from_an_interrupted_commit() {
    let dir = common::fresh_temp_dir("staging_restore_scan");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("save.bin"), b"keep me").unwrap();
    storage.build_local_index("f", OWN, &[]).unwrap();

    fs::rename(
        root.join("save.bin"),
        storage.download_backup_path("f", "save.bin"),
    )
    .unwrap();
    let (index, _) = storage.build_local_index("f", OWN, &[]).unwrap();
    let file = index.files.iter().find(|f| f.name == "save.bin").unwrap();
    assert!(!file.deleted, "restored file must not be announced as deleted");
    assert_eq!(fs::read(root.join("save.bin")).unwrap(), b"keep me");
    let _ = fs::remove_dir_all(&dir);
}

/// Leftovers of a download are never promoted to files, and unknown internal names are left
/// alone by a scan (no restore, no removal, no panic).
#[test]
fn test_scan_ignores_leftover_download_files_and_internal_names() {
    let dir = common::fresh_temp_dir("staging_leftovers");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    // A temp file of an unfinished download of `x`, whose `x` does not exist.
    fs::write(root.join(".syncthing.x.tmp"), b"partial").unwrap();
    fs::write(root.join(".stignore"), b"").unwrap();
    fs::write(root.join(".syncthing..bak"), b"odd").unwrap();
    fs::write(root.join(".syncthing.bak"), b"odd").unwrap();
    fs::create_dir_all(root.join(".stfolder")).unwrap();

    assert!(names(&storage, "f").is_empty(), "no internal name is indexed");
    assert!(!root.join("x").exists(), "a temp file is never promoted");
    assert_eq!(fs::read(root.join(".syncthing..bak")).unwrap(), b"odd");
    assert_eq!(fs::read(root.join(".syncthing.bak")).unwrap(), b"odd");
    assert!(root.join(".syncthing.x.tmp").is_file());
    let _ = fs::remove_dir_all(&dir);
}
