//! Block layout of local files, delta comparison against a peer's blocks, block reads served
//! to peers and the folder path helpers.

mod common;

use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use syncthing_core::bep::{BEP_BLOCK_SIZE, BEP_MAX_BLOCK_SIZE};
use syncthing_core::{BepBlockInfo, BepFileInfo, DeviceId, FsSyncStorage};

fn sha(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// Deterministic, non-repeating-at-block-size content.
fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| ((i / 7) as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// The block list a peer would announce for `data` (128 KiB blocks, one empty block if empty).
fn expected_blocks(data: &[u8]) -> Vec<BepBlockInfo> {
    if data.is_empty() {
        return vec![BepBlockInfo {
            offset: 0,
            size: 0,
            hash: sha(&[]),
            weak_hash: 0,
        }];
    }
    let mut out = Vec::new();
    let mut offset = 0i64;
    for chunk in data.chunks(BEP_BLOCK_SIZE) {
        out.push(BepBlockInfo {
            offset,
            size: chunk.len() as i32,
            hash: sha(chunk),
            weak_hash: syncthing_core::storage::weak_hash::weak_hash(chunk),
        });
        offset += chunk.len() as i64;
    }
    out
}

fn remote_of(name: &str, data: &[u8]) -> BepFileInfo {
    BepFileInfo {
        name: name.to_owned(),
        is_dir: false,
        size: data.len() as i64,
        modified_s: 1,
        deleted: false,
        invalid: false,
        sequence: 1,
        versions: Vec::new(),
        blocks: expected_blocks(data),
        ..Default::default()
    }
}

fn write(root: &Path, name: &str, data: &[u8]) {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, data).unwrap();
}

#[test]
fn test_block_layout_at_and_around_the_block_size() {
    let dir = common::fresh_temp_dir("block_layout");
    let storage = FsSyncStorage::new(&dir);
    let own = DeviceId([0x41; 32]);
    let root = storage.folder_root("f");
    let block = BEP_BLOCK_SIZE;
    let sizes = [0, 1, block - 1, block, block + 1, 2 * block, 2 * block + 1];
    for (i, len) in sizes.iter().enumerate() {
        write(&root, &format!("s{i}.bin"), &pattern(*len, i as u8));
    }

    let (index, _) = storage.build_local_index("f", own, &[]).unwrap();
    for (i, len) in sizes.iter().enumerate() {
        let name = format!("s{i}.bin");
        let f = index.files.iter().find(|f| f.name == name).unwrap();
        let data = pattern(*len, i as u8);
        assert_eq!(f.size, *len as i64, "size of {len} bytes");
        assert_eq!(f.blocks, expected_blocks(&data), "blocks of {len} bytes");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_delta_reuses_unchanged_blocks() {
    let dir = common::fresh_temp_dir("delta_reuse");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    let data = pattern(2 * BEP_BLOCK_SIZE + 100, 9);
    write(&root, "save.bin", &data);
    let remote = remote_of("save.bin", &data);

    // Identical content: nothing to download.
    assert!(storage
        .find_outdated_blocks("f", &remote)
        .unwrap()
        .is_empty());

    // Only the middle block changed on disk: only that block is outdated.
    let mut changed = data.clone();
    changed[BEP_BLOCK_SIZE + 5] ^= 0xff;
    write(&root, "save.bin", &changed);
    let outdated = storage.find_outdated_blocks("f", &remote).unwrap();
    assert_eq!(outdated, vec![remote.blocks[1].clone()]);

    // The peer changed only its last block: only that block is outdated.
    write(&root, "save.bin", &data);
    let mut newer = remote.clone();
    newer.blocks[2].hash = sha(b"something else");
    assert_eq!(
        storage.find_outdated_blocks("f", &newer).unwrap(),
        vec![newer.blocks[2].clone()]
    );

    // A local file cut short: the blocks it no longer covers are outdated.
    write(&root, "save.bin", &data[..BEP_BLOCK_SIZE + 10]);
    let outdated = storage.find_outdated_blocks("f", &remote).unwrap();
    assert_eq!(
        outdated,
        vec![remote.blocks[1].clone(), remote.blocks[2].clone()]
    );

    // No local file: every block is needed.
    let _ = fs::remove_file(root.join("save.bin"));
    assert_eq!(
        storage.find_outdated_blocks("f", &remote).unwrap(),
        remote.blocks
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A block larger than the 128 KiB reading buffer is hashed in full, not truncated or panicking.
#[test]
fn test_delta_handles_blocks_larger_than_the_read_buffer() {
    let dir = common::fresh_temp_dir("delta_big_block");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    let data = pattern(200 * 1024, 3);
    write(&root, "big.bin", &data);
    let remote = BepFileInfo {
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: data.len() as i32,
            hash: sha(&data),
            weak_hash: syncthing_core::storage::weak_hash::weak_hash(&data),
        }],
        ..remote_of("big.bin", &data)
    };
    assert!(storage
        .find_outdated_blocks("f", &remote)
        .unwrap()
        .is_empty());

    write(&root, "big.bin", &pattern(200 * 1024, 4));
    assert_eq!(
        storage.find_outdated_blocks("f", &remote).unwrap(),
        remote.blocks
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A block of exactly the BEP maximum size is accepted; one byte more is refused.
#[test]
fn test_block_size_limit_is_inclusive() {
    let dir = common::fresh_temp_dir("delta_max_block");
    let storage = FsSyncStorage::new(&dir);
    let size = BEP_MAX_BLOCK_SIZE as i64;
    let at_limit = BepFileInfo {
        name: "huge.bin".to_owned(),
        is_dir: false,
        size,
        modified_s: 1,
        deleted: false,
        invalid: false,
        sequence: 1,
        versions: Vec::new(),
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: BEP_MAX_BLOCK_SIZE as i32,
            hash: vec![0; 32],
            weak_hash: 0,
        }],
        ..Default::default()
    };
    // No local file: the announced blocks are returned without reading anything.
    let outdated = storage.find_outdated_blocks("f", &at_limit).unwrap();
    assert_eq!(outdated, at_limit.blocks);

    let over = BepFileInfo {
        size: size + 1,
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: BEP_MAX_BLOCK_SIZE as i32 + 1,
            hash: vec![0; 32],
            weak_hash: 0,
        }],
        ..at_limit
    };
    assert!(storage.find_outdated_blocks("f", &over).is_err());
    let _ = fs::remove_dir_all(&dir);
}

/// Reads of a block for a peer return exactly the requested bytes, the maximum size included.
#[test]
fn test_read_block_for_peer_returns_the_requested_range() {
    let dir = common::fresh_temp_dir("read_block_peer");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    fs::create_dir_all(&root).unwrap();
    // A sparse file of exactly the maximum block size, with a marker inside.
    let mut file = File::create(root.join("big.bin")).unwrap();
    file.set_len(BEP_MAX_BLOCK_SIZE as u64).unwrap();
    file.seek(SeekFrom::Start(1000)).unwrap();
    file.write_all(b"MARK").unwrap();
    drop(file);

    assert_eq!(
        storage
            .read_block_for_peer("f", "big.bin", 1000, 4)
            .unwrap(),
        b"MARK"
    );
    let whole = storage
        .read_block_for_peer("f", "big.bin", 0, BEP_MAX_BLOCK_SIZE)
        .expect("a block of exactly the maximum size is served");
    assert_eq!(whole.len(), BEP_MAX_BLOCK_SIZE);
    assert!(storage
        .read_block_for_peer("f", "big.bin", 0, BEP_MAX_BLOCK_SIZE + 1)
        .is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_folder_root_keeps_only_safe_id_characters() {
    let storage = FsSyncStorage::new(Path::new("base"));
    assert_eq!(
        storage.folder_root("a-b_c.d/../e"),
        PathBuf::from("base").join("a-b_cde")
    );
    assert_eq!(
        storage.folder_root("xkx75-ukkm3"),
        PathBuf::from("base").join("xkx75-ukkm3")
    );
    assert_eq!(storage.folder_root("..."), PathBuf::from("base").join(""));
    let db = storage.db_path("a-b_c.d/../e");
    assert_eq!(db.parent(), Some(Path::new("base")));
    assert!(db
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with(".syncthing_db_a-b_cde_"));
}

/// An empty file has one zero-length block; it is a valid block, not an out-of-bounds one.
#[test]
fn test_zero_length_block_of_an_empty_file_is_accepted() {
    let dir = common::fresh_temp_dir("delta_empty_block");
    let storage = FsSyncStorage::new(&dir);
    let remote = BepFileInfo {
        name: "empty.bin".to_owned(),
        is_dir: false,
        size: 0,
        modified_s: 1,
        deleted: false,
        invalid: false,
        sequence: 1,
        versions: Vec::new(),
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: 0,
            hash: sha(&[]),
            weak_hash: 0,
        }],
        ..Default::default()
    };
    assert_eq!(
        storage.find_outdated_blocks("f", &remote).unwrap(),
        remote.blocks
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_custom_folder_path_wins_and_moves_the_database() {
    let dir = common::fresh_temp_dir("custom_root");
    let mut storage = FsSyncStorage::new(dir.join("default"));
    let default_db = storage.db_path("f");
    assert!(default_db.starts_with(dir.join("default")));
    assert!(default_db
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with(".syncthing_db_f_"));

    let custom = dir.join("custom");
    fs::create_dir_all(&custom).unwrap();
    storage.set_folder_path("f", &custom);
    assert_eq!(storage.folder_root("f"), custom);
    assert_ne!(
        storage.db_path("f"),
        default_db,
        "a new root starts a new database"
    );
    assert_eq!(
        storage.resolve_safe_path("f", "../a/./b\\c"),
        custom.join("a").join("b").join("c")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_a_block_hash_that_is_not_a_sha256_is_rejected_even_for_identical_content() {
    let dir = common::fresh_temp_dir("delta_bad_hash_len");
    let storage = FsSyncStorage::new(&dir);
    let root = storage.folder_root("f");
    let data = pattern(BEP_BLOCK_SIZE + 10, 4);
    write(&root, "save.bin", &data);

    for bad_hash in [
        Vec::new(),
        sha(&data[..BEP_BLOCK_SIZE])[..31].to_vec(),
        vec![0; 33],
    ] {
        let mut remote = remote_of("save.bin", &data);
        // Truncated, empty or oversized: the local block can never be verified against it.
        remote.blocks[0].hash = bad_hash.clone();
        let err = storage
            .find_outdated_blocks("f", &remote)
            .expect_err("an unverifiable block must not be taken as up to date");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            err.to_string()
                .contains(&format!("{}-byte hash", bad_hash.len())),
            "{err}"
        );
    }
    assert_eq!(
        fs::read(root.join("save.bin")).unwrap(),
        data,
        "the local file is left untouched"
    );
}
