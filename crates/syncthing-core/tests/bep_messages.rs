//! BEP/1.0 message codecs with hand-built byte vectors: `Header`, `Close`, `Request`,
//! `Response` and `ClusterConfig` (folders, devices, index state).

use syncthing_core::proto::{encode_bytes_field, WireIter};
use syncthing_core::{
    BepClose, BepClusterConfig, BepCompression, BepFolder, BepHeader, BepMessageType, BepRequest,
    BepResponse, DeviceId, FolderIndexState, FolderType,
};

// ---------------------------------------------------------------- Header

#[test]
fn header_encodes_type_and_compression_fields() {
    let plain = BepHeader {
        msg_type: BepMessageType::Index,
        compression: BepCompression::None,
    };
    assert_eq!(plain.encode(), vec![0x08, 0x01]);

    let lz4 = BepHeader {
        msg_type: BepMessageType::Index,
        compression: BepCompression::Lz4,
    };
    assert_eq!(lz4.encode(), vec![0x08, 0x01, 0x10, 0x01]);

    // ClusterConfig (type 0) is the proto3 default: no type field is written.
    let cluster = BepHeader {
        msg_type: BepMessageType::ClusterConfig,
        compression: BepCompression::None,
    };
    assert!(cluster.encode().is_empty());
}

#[test]
fn header_decodes_compression_flag_exactly() {
    let lz4 = BepHeader::decode(&[0x10, 0x01]).unwrap();
    assert_eq!(lz4.compression, BepCompression::Lz4);
    assert_eq!(lz4.msg_type, BepMessageType::ClusterConfig);

    // Only the value 1 means LZ4; 0 and other values are uncompressed.
    let none = BepHeader::decode(&[0x10, 0x00]).unwrap();
    assert_eq!(none.compression, BepCompression::None);
    let other = BepHeader::decode(&[0x10, 0x02]).unwrap();
    assert_eq!(other.compression, BepCompression::None);

    let close = BepHeader::decode(&[0x08, 0x07]).unwrap();
    assert_eq!(close.msg_type, BepMessageType::Close);
    assert_eq!(close.compression, BepCompression::None);

    let unknown = BepHeader::decode(&[0x08, 0x63]).unwrap();
    assert_eq!(unknown.msg_type, BepMessageType::Unknown(99));
}

#[test]
fn message_type_numbers_roundtrip() {
    for v in 0..=7u64 {
        assert_eq!(BepMessageType::from(v).as_u64(), v);
    }
    assert_eq!(BepMessageType::from(8), BepMessageType::Unknown(8));
    assert_eq!(BepMessageType::Unknown(8).as_u64(), 8);
}

// ---------------------------------------------------------------- Close

#[test]
fn close_encodes_reason_as_field_one() {
    let close = BepClose {
        reason: "bye".to_owned(),
    };
    assert_eq!(close.encode(), vec![0x0A, 0x03, b'b', b'y', b'e']);
    let empty = BepClose {
        reason: String::new(),
    };
    assert!(empty.encode().is_empty());
}

#[test]
fn close_decodes_only_length_delimited_field_one() {
    let decoded = BepClose::decode(&[0x0A, 0x03, b'b', b'y', b'e']).unwrap();
    assert_eq!(decoded.reason, "bye");

    // A varint field 1 is not the reason string.
    let varint_field = BepClose::decode(&[0x08, 0x05]).unwrap();
    assert_eq!(varint_field.reason, "");

    // A length-delimited field 2 is ignored.
    let other_field = BepClose::decode(&[0x12, 0x01, b'x']).unwrap();
    assert_eq!(other_field.reason, "");
}

// ---------------------------------------------------------------- Request

fn sample_request() -> BepRequest {
    BepRequest {
        id: 1,
        folder: "f".to_owned(),
        name: "a".to_owned(),
        offset: 131_072,
        size: 131_072,
        hash: vec![0xAB],
        weak_hash: 0,
    }
}

#[test]
fn request_encodes_exact_field_layout() {
    // offset and size = 2^17 = varint [0x80, 0x80, 0x08]; hash = field 6 (tag 0x32).
    let expected = vec![
        0x08, 0x01, // id = 1
        0x12, 0x01, b'f', // folder
        0x1A, 0x01, b'a', // name
        0x20, 0x80, 0x80, 0x08, // offset (field 4)
        0x28, 0x80, 0x80, 0x08, // size (field 5)
        0x32, 0x01, 0xAB, // hash (field 6)
    ];
    assert_eq!(sample_request().encode(), expected);
}

#[test]
fn request_decodes_offset_size_and_hash() {
    let bytes = sample_request().encode();
    let decoded = BepRequest::decode(&bytes).unwrap();
    assert_eq!(decoded, sample_request());
    assert_eq!(decoded.offset, 131_072);
    assert_eq!(decoded.size, 131_072);
    assert_eq!(decoded.hash, vec![0xAB]);
}

#[test]
fn request_omits_zero_offset_size_and_empty_hash() {
    let req = BepRequest {
        id: 2,
        folder: String::new(),
        name: String::new(),
        offset: 0,
        size: 0,
        hash: Vec::new(),
        weak_hash: 0,
    };
    // Only id (field 1), folder and name (always written, even when empty) are encoded.
    assert_eq!(req.encode(), vec![0x08, 0x02, 0x12, 0x00, 0x1A, 0x00]);
}

// ---------------------------------------------------------------- Response

#[test]
fn response_encodes_and_decodes_code_and_data() {
    let response = BepResponse {
        id: 2,
        data: vec![1, 2],
        code: 5,
    };
    let bytes = response.encode();
    assert_eq!(bytes, vec![0x08, 0x02, 0x12, 0x02, 1, 2, 0x18, 0x05]);
    assert_eq!(BepResponse::decode(&bytes).unwrap(), response);

    let ok = BepResponse {
        id: 3,
        data: Vec::new(),
        code: 0,
    };
    assert_eq!(ok.encode(), vec![0x08, 0x03]);
}

// ---------------------------------------------------------------- ClusterConfig

/// Builds a length-delimited device entry (`Device` message) with the given field.
fn device_entry(field: u32, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, field, payload);
    buf
}

/// Builds a `ClusterConfig` folder entry (field 1 of the message) by hand.
fn folder_message(devices: &[Vec<u8>]) -> Vec<u8> {
    let mut folder = Vec::new();
    encode_bytes_field(&mut folder, 1, b"f");
    encode_bytes_field(&mut folder, 2, b"Lbl");
    for dev in devices {
        encode_bytes_field(&mut folder, 16, dev);
    }
    let mut message = Vec::new();
    encode_bytes_field(&mut message, 1, &folder);
    message
}

#[test]
fn cluster_config_decodes_label_and_device_ids() {
    let id_a = [0x11u8; 32];
    let id_b = [0x22u8; 32];
    let dev_a = device_entry(1, &id_a);
    let dev_b = device_entry(1, &id_b);
    let message = folder_message(&[dev_a, dev_b]);

    let decoded = BepClusterConfig::decode(&message).unwrap();
    assert_eq!(decoded.folders.len(), 1);
    let folder = &decoded.folders[0];
    assert_eq!(folder.id, "f");
    assert_eq!(folder.label, "Lbl");
    assert_eq!(
        folder.device_ids,
        vec![DeviceId(id_a), DeviceId(id_b)],
        "both device entries carry 32-byte IDs"
    );
}

#[test]
fn cluster_config_ignores_device_fields_that_are_not_ids() {
    // Field 2 (name) with 32 bytes, and an ID of 31 bytes, are not device IDs.
    let not_id_field = device_entry(2, &[0x33u8; 32]);
    let short_id = device_entry(1, &[0x44u8; 31]);
    let real = device_entry(1, &[0x55u8; 32]);
    let message = folder_message(&[not_id_field, short_id, real]);

    let decoded = BepClusterConfig::decode(&message).unwrap();
    assert_eq!(decoded.folders[0].device_ids, vec![DeviceId([0x55; 32])]);
}

#[test]
fn cluster_config_decodes_secondary_flag_exactly() {
    assert_eq!(BepClusterConfig::encode_secondary(), vec![0x10, 0x01]);
    let primary = BepClusterConfig::decode(&[]).unwrap();
    assert!(!primary.secondary);
    assert!(primary.folders.is_empty());
}

fn own_device() -> DeviceId {
    DeviceId([0x11; 32])
}

fn peer_device() -> DeviceId {
    DeviceId([0x22; 32])
}

fn folder(id: &str, label: &str) -> BepFolder {
    BepFolder {
        id: id.to_owned(),
        label: label.to_owned(),
        device_ids: Vec::new(),
        ..Default::default()
    }
}

/// Returns the field-16 device entries of the (single) folder of an encoded `ClusterConfig`.
fn device_entries_of(message: &[u8]) -> Vec<Vec<u8>> {
    let folder_bytes = WireIter::new(message)
        .map(Result::unwrap)
        .find(|(field, _, _)| *field == 1)
        .expect("folder entry")
        .2;
    WireIter::new(folder_bytes)
        .map(Result::unwrap)
        .filter(|(field, _, _)| *field == 16)
        .map(|(_, _, data)| data.to_vec())
        .collect()
}

/// Returns the varint value of `field` inside an encoded device entry, if present.
fn device_varint(entry: &[u8], field: u32) -> Option<u64> {
    WireIter::new(entry)
        .map(Result::unwrap)
        .find(|(f, wire, _)| *f == field && *wire == 0)
        .map(|(_, _, data)| syncthing_core::proto::decode_varint(data).unwrap().0)
}

#[test]
fn encode_for_peers_writes_own_index_state_only_when_set() {
    let state = FolderIndexState {
        index_id: 0x1234,
        max_sequence: 5,
        paused: false,
        folder_type: FolderType::SendReceive,
    };
    let message = BepClusterConfig::encode_for_peers(
        &[(folder("f", ""), state)],
        own_device(),
        "vita",
        peer_device(),
    );
    let devices = device_entries_of(&message);
    assert_eq!(devices.len(), 2, "own device then the remote peer");
    assert_eq!(device_varint(&devices[0], 6), Some(5), "max_sequence");
    assert_eq!(device_varint(&devices[0], 8), Some(0x1234), "index_id");
    assert_eq!(
        device_varint(&devices[0], 4),
        None,
        "compression metadata (proto3 default)"
    );
    assert_eq!(device_varint(&devices[1], 6), None);
    assert_eq!(device_varint(&devices[1], 8), None);

    let fresh = FolderIndexState::default();
    let message = BepClusterConfig::encode_for_peers(
        &[(folder("f", ""), fresh)],
        own_device(),
        "",
        peer_device(),
    );
    let devices = device_entries_of(&message);
    assert_eq!(device_varint(&devices[0], 6), None, "zero sequence omitted");
    assert_eq!(device_varint(&devices[0], 8), None, "zero index id omitted");
}

#[test]
fn encode_for_peers_writes_folder_type_and_pause() {
    let state = FolderIndexState {
        index_id: 0,
        max_sequence: 0,
        paused: true,
        folder_type: FolderType::ReceiveOnly,
    };
    let message = BepClusterConfig::encode_for_peers(
        &[(folder("f", "Label"), state)],
        own_device(),
        "vita",
        peer_device(),
    );
    let folder_bytes = WireIter::new(&message).next().unwrap().unwrap().2.to_vec();
    let fields: Vec<(u32, u8, Vec<u8>)> = WireIter::new(&folder_bytes)
        .map(|item| {
            let (f, w, d) = item.unwrap();
            (f, w, d.to_vec())
        })
        .filter(|(f, _, _)| *f != 16)
        .collect();
    assert_eq!(
        fields,
        vec![
            (1, 2, b"f".to_vec()),
            (2, 2, b"Label".to_vec()),
            (3, 0, vec![2]), // RECEIVE_ONLY
            (7, 0, vec![1]), // paused
        ]
    );

    let decoded = BepClusterConfig::decode(&message).unwrap();
    assert_eq!(decoded.folders[0].label, "Label");
    assert_eq!(
        decoded.folders[0].device_ids,
        vec![own_device(), peer_device()]
    );
}
