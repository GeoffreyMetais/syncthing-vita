//! Relay Protocol v1 XDR wire format (`relay::wire`): exact bytes of an encoded request
//! (opaque padding to 4 bytes), and truncated XDR payloads rejected without a panic.

use std::io;

use syncthing_core::relay::wire::{decode_payload, encode_join_session_request, RelayMsgType};

#[test]
fn an_opaque_key_is_padded_with_zeros_to_a_multiple_of_four() {
    // Frame: magic 0x9E79BC40, type 3 (JoinSessionRequest), payload length 12.
    // Payload: opaque length 5, the five key bytes, then 3 zero bytes of padding.
    let frame = encode_join_session_request(&[1, 2, 3, 4, 5]);
    assert_eq!(
        frame,
        [
            0x9E, 0x79, 0xBC, 0x40, // magic
            0x00, 0x00, 0x00, 0x03, // message type
            0x00, 0x00, 0x00, 0x0C, // payload length
            0x00, 0x00, 0x00, 0x05, // opaque length
            0x01, 0x02, 0x03, 0x04, 0x05, // key
            0x00, 0x00, 0x00, // padding
        ]
    );
}

#[test]
fn an_opaque_key_of_a_multiple_of_four_has_no_padding() {
    let frame = encode_join_session_request(&[9, 8, 7, 6]);
    assert_eq!(
        frame,
        [
            0x9E, 0x79, 0xBC, 0x40, // magic
            0x00, 0x00, 0x00, 0x03, // message type
            0x00, 0x00, 0x00, 0x08, // payload length
            0x00, 0x00, 0x00, 0x04, // opaque length
            0x09, 0x08, 0x07, 0x06, // key
        ]
    );
}

#[test]
fn a_truncated_xdr_u32_is_an_eof_error_not_a_panic() {
    // A Response needs a 4-byte code: two bytes are not enough.
    let err = decode_payload(RelayMsgType::Response, &[0, 0]).expect_err("truncated code");
    assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "{err}");

    // The code is complete but the message length is cut.
    let err =
        decode_payload(RelayMsgType::Response, &[0, 0, 0, 0, 0, 0]).expect_err("truncated length");
    assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "{err}");
}

#[test]
fn a_response_with_a_padded_message_decodes_exactly() {
    // Code 1, message "no" (2 bytes) padded with two zero bytes.
    let payload = [0, 0, 0, 1, 0, 0, 0, 2, b'n', b'o', 0, 0];
    let decoded = decode_payload(RelayMsgType::Response, &payload).expect("response");
    assert_eq!(
        decoded,
        syncthing_core::RelayMessage::Response(syncthing_core::RelayResponse {
            code: 1,
            message: "no".to_owned(),
        })
    );
}
