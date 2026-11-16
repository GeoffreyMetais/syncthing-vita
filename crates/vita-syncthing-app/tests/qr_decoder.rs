//! `QrDecoder`: payloads of QR codes found in greyscale camera frames, host-side (no camera).

use qrcodegen::{QrCode, QrCodeEcc};
use vita_syncthing_app::infrastructure::{QrDecoder, CAMERA_H, CAMERA_W};

/// Renders `qr` (4 px per module) centred on a white frame of `width` x `height`.
fn frame_with(qr: &QrCode, width: usize, height: usize) -> Vec<u8> {
    let scale = 4;
    let size = (qr.size() * scale) as usize;
    let ox = (width - size) / 2;
    let oy = (height - size) / 2;
    let mut gray = vec![255u8; width * height];
    for y in 0..size {
        for x in 0..size {
            if qr.get_module((x / scale as usize) as i32, (y / scale as usize) as i32) {
                gray[(oy + y) * width + ox + x] = 0;
            }
        }
    }
    gray
}

fn camera_frame_with_text(text: &str) -> Vec<u8> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).unwrap();
    frame_with(&qr, CAMERA_W, CAMERA_H)
}

#[test]
fn test_decodes_the_text_of_a_rendered_code() {
    let payload = "SYNCTHING-TEST-PAYLOAD-0123";
    let frame = camera_frame_with_text(payload);
    let mut decoder = QrDecoder::new();
    assert_eq!(
        decoder.decode(&frame, CAMERA_W, CAMERA_H),
        vec![payload.to_owned()]
    );
}

#[test]
fn test_decoder_is_reusable_between_frames() {
    let mut decoder = QrDecoder::default();
    let first = camera_frame_with_text("FIRST");
    let second = camera_frame_with_text("SECOND");
    assert_eq!(decoder.decode(&first, CAMERA_W, CAMERA_H), vec!["FIRST"]);
    assert_eq!(decoder.decode(&second, CAMERA_W, CAMERA_H), vec!["SECOND"]);
    let blank = vec![255u8; CAMERA_W * CAMERA_H];
    assert!(decoder.decode(&blank, CAMERA_W, CAMERA_H).is_empty());
}

#[test]
fn test_only_the_width_by_height_area_is_read() {
    let frame = camera_frame_with_text("PADDED");
    // Extra bytes after the frame are ignored.
    let mut padded = frame.clone();
    padded.extend_from_slice(&[0u8; 1000]);
    let mut decoder = QrDecoder::new();
    assert_eq!(decoder.decode(&padded, CAMERA_W, CAMERA_H), vec!["PADDED"]);
}

#[test]
fn test_frame_smaller_than_its_dimensions_is_rejected() {
    let frame = camera_frame_with_text("TRUNCATED");
    let mut decoder = QrDecoder::new();
    assert!(decoder
        .decode(&frame[..frame.len() - 1], CAMERA_W, CAMERA_H)
        .is_empty());
    assert!(decoder.decode(&[], CAMERA_W, CAMERA_H).is_empty());
}

#[test]
fn test_zero_sized_frames_are_rejected() {
    let frame = camera_frame_with_text("ZERO");
    let mut decoder = QrDecoder::new();
    assert!(decoder.decode(&frame, 0, CAMERA_H).is_empty());
    assert!(decoder.decode(&frame, CAMERA_W, 0).is_empty());
    assert!(decoder.decode(&[], 0, 0).is_empty());
}

#[test]
fn test_blank_frame_yields_no_payload() {
    let blank = vec![255u8; CAMERA_W * CAMERA_H];
    assert!(QrDecoder::new()
        .decode(&blank, CAMERA_W, CAMERA_H)
        .is_empty());
}

#[test]
fn test_payload_that_is_not_utf8_is_skipped() {
    let qr = QrCode::encode_binary(&[0xff, 0xfe, 0x80, 0xc3], QrCodeEcc::Medium).unwrap();
    let frame = frame_with(&qr, CAMERA_W, CAMERA_H);
    assert!(QrDecoder::new()
        .decode(&frame, CAMERA_W, CAMERA_H)
        .is_empty());
}
