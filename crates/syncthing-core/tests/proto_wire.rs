//! Protobuf wire primitives: varint boundary encodings, field tags, `WireIter` bounds.

use std::io::ErrorKind;

use syncthing_core::proto::{
    decode_varint, encode_bytes_field, encode_varint, encode_varint_field, WireIter,
};

fn varint(v: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_varint(v, &mut buf);
    buf
}

#[test]
fn varint_encodes_boundary_values_exactly() {
    assert_eq!(varint(0), vec![0x00]);
    assert_eq!(varint(1), vec![0x01]);
    assert_eq!(varint(127), vec![0x7F]);
    assert_eq!(varint(128), vec![0x80, 0x01]);
    assert_eq!(varint(300), vec![0xAC, 0x02]);
    assert_eq!(varint(16_383), vec![0xFF, 0x7F]);
    assert_eq!(varint(16_384), vec![0x80, 0x80, 0x01]);
    assert_eq!(varint(1 << 63), [vec![0x80; 9], vec![0x01]].concat());
    assert_eq!(varint(u64::MAX), [vec![0xFF; 9], vec![0x01]].concat());
}

#[test]
fn varint_decodes_boundary_values_and_reports_length() {
    assert_eq!(decode_varint(&[0x00]).unwrap(), (0, 1));
    assert_eq!(decode_varint(&[0x7F]).unwrap(), (127, 1));
    assert_eq!(decode_varint(&[0x80, 0x01]).unwrap(), (128, 2));
    assert_eq!(decode_varint(&[0xFF, 0x7F]).unwrap(), (16_383, 2));
    assert_eq!(decode_varint(&[0x80, 0x80, 0x01]).unwrap(), (16_384, 3));
    // Trailing bytes after the varint are not consumed.
    assert_eq!(decode_varint(&[0x7F, 0xFF]).unwrap(), (127, 1));
    let max = [vec![0xFF; 9], vec![0x01]].concat();
    assert_eq!(decode_varint(&max).unwrap(), (u64::MAX, 10));
    let two_pow_63 = [vec![0x80; 9], vec![0x01]].concat();
    assert_eq!(decode_varint(&two_pow_63).unwrap(), (1 << 63, 10));
}

#[test]
fn varint_roundtrip_over_powers_of_two() {
    for shift in 0..64 {
        for v in [(1u64 << shift) - 1, 1u64 << shift] {
            let bytes = varint(v);
            assert_eq!(decode_varint(&bytes).unwrap(), (v, bytes.len()));
        }
    }
}

#[test]
fn varint_rejects_truncated_and_oversized_input() {
    let truncated = decode_varint(&[0x80]).unwrap_err();
    assert_eq!(truncated.kind(), ErrorKind::UnexpectedEof);
    assert!(decode_varint(&[]).is_err());
    // Eleven continuation bytes exceed 64 bits.
    let oversized = [vec![0xFF; 10], vec![0x01]].concat();
    assert_eq!(
        decode_varint(&oversized).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn field_encoders_write_exact_tags_and_payloads() {
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, 1, b"ab");
    assert_eq!(buf, vec![0x0A, 0x02, b'a', b'b']);

    // Field 16 with wire type 2: tag = (16 << 3) | 2 = 130 = varint [0x82, 0x01].
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, 16, &[]);
    assert_eq!(buf, vec![0x82, 0x01, 0x00]);

    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 4, 0x2A);
    assert_eq!(buf, vec![0x20, 0x2A]);

    // Field 2 with wire type 0 and a 64-bit value: tag 0x10 then varint.
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 2, 1 << 63);
    assert_eq!(buf[0], 0x10);
    assert_eq!(&buf[1..], &[vec![0x80; 9], vec![0x01]].concat()[..]);
}

#[test]
fn wire_iter_decodes_each_wire_type() {
    // field 1 varint 150
    let varint_field = [0x08, 0x96, 0x01];
    let items: Vec<_> = WireIter::new(&varint_field).map(Result::unwrap).collect();
    assert_eq!(items, vec![(1, 0, &[0x96, 0x01][..])]);

    // field 2 length-delimited "abc"
    let bytes_field = [0x12, 0x03, b'a', b'b', b'c'];
    let items: Vec<_> = WireIter::new(&bytes_field).map(Result::unwrap).collect();
    assert_eq!(items, vec![(2, 2, &b"abc"[..])]);

    // field 1 fixed64, field 2 fixed32
    let fixed = [0x09, 1, 2, 3, 4, 5, 6, 7, 8, 0x15, 9, 10, 11, 12];
    let items: Vec<_> = WireIter::new(&fixed).map(Result::unwrap).collect();
    assert_eq!(
        items,
        vec![
            (1, 1, &[1, 2, 3, 4, 5, 6, 7, 8][..]),
            (2, 5, &[9, 10, 11, 12][..]),
        ]
    );
}

#[test]
fn wire_iter_accepts_exact_fixed_width_fields() {
    // `count()` alone would also count an `Err` item: every item must be `Ok`.
    let fixed64 = [0x09, 1, 2, 3, 4, 5, 6, 7, 8];
    let items: Vec<_> = WireIter::new(&fixed64).map(Result::unwrap).collect();
    assert_eq!(items, vec![(1, 1, &[1, 2, 3, 4, 5, 6, 7, 8][..])]);
    let fixed32 = [0x15, 1, 2, 3, 4];
    let items: Vec<_> = WireIter::new(&fixed32).map(Result::unwrap).collect();
    assert_eq!(items, vec![(2, 5, &[1, 2, 3, 4][..])]);
    let bytes = [0x0A, 0x00];
    let items: Vec<_> = WireIter::new(&bytes).map(Result::unwrap).collect();
    assert_eq!(items, vec![(1, 2, &[][..])]);
}

#[test]
fn wire_iter_rejects_truncated_fixed_width_fields_without_panic() {
    // 64-bit field with only 7 of its 8 bytes.
    let short64 = [0x09, 1, 2, 3, 4, 5, 6, 7];
    let err = WireIter::new(&short64).next().unwrap().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);

    // 64-bit field with 5 bytes (the length check must not be an equality).
    let short64b = [0x09, 1, 2, 3, 4];
    assert!(WireIter::new(&short64b).next().unwrap().is_err());

    // 32-bit field with only 3 of its 4 bytes.
    let short32 = [0x15, 1, 2, 3];
    let err = WireIter::new(&short32).next().unwrap().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);

    // 32-bit field with a single byte.
    let short32b = [0x15, 1];
    assert!(WireIter::new(&short32b).next().unwrap().is_err());
}

#[test]
fn wire_iter_rejects_truncated_length_delimited_field() {
    let truncated = [0x12, 0x05, b'a', b'b'];
    let err = WireIter::new(&truncated).next().unwrap().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
}

#[test]
fn wire_iter_rejects_unsupported_wire_types_and_stops() {
    // Wire types 3, 4, 6 and 7 are not part of proto3 messages used by BEP.
    for wire in [3u8, 4, 6, 7] {
        let bytes = [(1 << 3) | wire, 0x00, 0x08, 0x01];
        let mut iter = WireIter::new(&bytes);
        let err = iter.next().unwrap().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
        assert!(iter.next().is_none(), "iterator must stop after an error");
    }
}

#[test]
fn wire_iter_reports_truncated_tag_and_stops() {
    let mut iter = WireIter::new(&[0x80]);
    assert_eq!(
        iter.next().unwrap().unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
    assert!(iter.next().is_none());
}

#[test]
fn wire_iter_empty_buffer_yields_nothing() {
    assert!(WireIter::new(&[]).next().is_none());
}
