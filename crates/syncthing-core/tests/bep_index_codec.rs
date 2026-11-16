//! `Index` / `IndexUpdate` codec: every file and block field survives an encode / decode
//! cycle, and the encoded file fields use the BEP field numbers.

use syncthing_core::proto::{decode_varint, WireIter};
use syncthing_core::{BepBlockInfo, BepFileInfo, BepIndex, BepVersionCounter};

fn file(name: &str) -> BepFileInfo {
    BepFileInfo {
        name: name.to_owned(),
        is_dir: false,
        size: 5,
        modified_s: 1_700_000_000,
        deleted: false,
        invalid: false,
        sequence: 7,
        versions: vec![BepVersionCounter {
            short_device_id: 0x0102_0304_0506_0708,
            value: 3,
        }],
        blocks: vec![BepBlockInfo {
            offset: 0,
            size: 5,
            hash: vec![0xEE; 32],
        }],
    }
}

fn index_of(files: Vec<BepFileInfo>) -> BepIndex {
    BepIndex {
        folder: "photos".to_owned(),
        files,
    }
}

/// Returns the bytes of the `n`-th top-level field 2 (a file entry) of an encoded `Index`.
fn file_entry(encoded: &[u8]) -> &[u8] {
    WireIter::new(encoded)
        .map(Result::unwrap)
        .filter(|(field, _, _)| *field == 2)
        .map(|(_, _, data)| data)
        .next()
        .expect("file entry")
}

#[test]
fn index_roundtrip_keeps_every_file_field() {
    let index = index_of(vec![file("a.txt")]);
    let decoded = BepIndex::decode(&index.encode()).unwrap();
    assert_eq!(decoded, index);
}

#[test]
fn index_roundtrip_keeps_sequence_deleted_invalid_and_block_offsets() {
    let mut deleted = file("gone.bin");
    deleted.deleted = true;
    deleted.size = 0;
    deleted.blocks.clear();
    deleted.sequence = 42;

    let mut multi = file("big.bin");
    multi.size = 2 * 65_536 + 9;
    multi.blocks = vec![
        BepBlockInfo {
            offset: 0,
            size: 65_536,
            hash: vec![1; 32],
        },
        BepBlockInfo {
            offset: 65_536,
            size: 65_536,
            hash: vec![2; 32],
        },
        BepBlockInfo {
            offset: 131_072,
            size: 9,
            hash: vec![3; 32],
        },
    ];

    let mut invalid = file("bad name");
    invalid.invalid = true;
    invalid.versions.clear();

    let index = index_of(vec![deleted.clone(), multi.clone(), invalid.clone()]);
    let decoded = BepIndex::decode(&index.encode()).unwrap();
    assert_eq!(decoded.files, vec![deleted, multi, invalid]);
    assert!(decoded.files[0].deleted);
    assert_eq!(decoded.files[0].sequence, 42);
    assert_eq!(decoded.files[1].blocks[2].offset, 131_072);
    assert_eq!(decoded.files[1].blocks[1].offset, 65_536);
}

#[test]
fn index_file_entry_uses_bep_field_numbers() {
    let encoded = index_of(vec![file("a")]).encode();

    // Folder is field 1 (length-delimited), each file is field 2.
    assert_eq!(&encoded[..8], &[0x0A, 0x06, b'p', b'h', b'o', b't', b'o', b's']);
    let entry = file_entry(&encoded);
    let fields: Vec<u32> = WireIter::new(entry).map(|item| item.unwrap().0).collect();
    // name(1), size(3), permissions(4), modified(5), no_permissions(8), versions(9),
    // sequence(10), modified_by(12), block_size(13), blocks(16). Field 2 is only for dirs.
    assert_eq!(fields, vec![1, 3, 4, 5, 8, 9, 10, 12, 13, 16]);
}

#[test]
fn index_file_entry_values_are_exact() {
    let encoded = index_of(vec![file("a")]).encode();
    let entry = file_entry(&encoded);
    let varint_of = |field: u32| -> u64 {
        let data = WireIter::new(entry)
            .map(Result::unwrap)
            .find(|(f, w, _)| *f == field && *w == 0)
            .unwrap_or_else(|| panic!("field {field} missing"))
            .2;
        decode_varint(data).unwrap().0
    };
    assert_eq!(varint_of(3), 5, "size");
    assert_eq!(varint_of(4), 0o644, "permissions");
    assert_eq!(varint_of(5), 1_700_000_000, "modified seconds");
    assert_eq!(varint_of(8), 1, "no_permissions");
    assert_eq!(varint_of(10), 7, "sequence");
    assert_eq!(varint_of(12), 0x0102_0304_0506_0708, "modified_by");
    assert_eq!(varint_of(13), 128 * 1024, "block size");
}

#[test]
fn index_directory_entry_has_directory_type_and_no_blocks() {
    let mut dir = file("sub");
    dir.is_dir = true;
    dir.blocks.clear();
    dir.versions.clear();
    dir.sequence = 0;
    dir.modified_s = 0;
    let encoded = index_of(vec![dir]).encode();
    let entry = file_entry(&encoded);
    let fields: Vec<u32> = WireIter::new(entry).map(|item| item.unwrap().0).collect();
    assert_eq!(fields, vec![1, 2, 4, 8]);
    let decoded = BepIndex::decode(&encoded).unwrap();
    assert!(decoded.files[0].is_dir);
    assert!(!decoded.files[0].invalid);
}

#[test]
fn index_decode_skips_unknown_fields_and_wrong_wire_types() {
    use syncthing_core::proto::{encode_bytes_field, encode_varint_field};
    let mut file_bytes = Vec::new();
    encode_bytes_field(&mut file_bytes, 1, b"x");
    encode_varint_field(&mut file_bytes, 99, 5); // unknown field
    encode_varint_field(&mut file_bytes, 1, 9); // name with the wrong wire type
    let mut index = Vec::new();
    encode_bytes_field(&mut index, 1, b"f");
    encode_bytes_field(&mut index, 2, &file_bytes);
    let decoded = BepIndex::decode(&index).unwrap();
    assert_eq!(decoded.folder, "f");
    assert_eq!(decoded.files[0].name, "x");
}
