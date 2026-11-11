//! QR code device pairing: payload parsing, `quircs` decoding of a rendered QR code, scanner
//! modal navigation and adding the scanned device.

mod common;

use common::temp_test_dir;
use qrcodegen::{QrCode, QrCodeEcc};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{
    extract_device_id, InputController, QrScanFlow, UserCommand,
};
use vita_syncthing_app::domain::{AppState, ModalDialog};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, QrDecoder, QrScanEvent, BTN_CIRCLE, BTN_R, CAMERA_H, CAMERA_W,
};

fn sample_device_id() -> String {
    DeviceId([7u8; 32]).to_syncthing_string()
}

fn press(state: &mut AppState, repo: &ConfigRepository, buttons: u32) -> UserCommand {
    InputController::handle_input(
        state,
        repo,
        InputFrame {
            pressed: buttons,
            any_activity: true,
            ..Default::default()
        },
    )
}

/// Renders `text` as a QR code (4 px per module) centred on a white camera-sized frame.
fn render_qr_frame(text: &str) -> Vec<u8> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).unwrap();
    let scale = 4;
    let size = qr.size() * scale;
    let ox = (CAMERA_W as i32 - size) / 2;
    let oy = (CAMERA_H as i32 - size) / 2;
    let mut gray = vec![255u8; CAMERA_W * CAMERA_H];
    for y in 0..size {
        for x in 0..size {
            if qr.get_module(x / scale, y / scale) {
                gray[((oy + y) as usize) * CAMERA_W + (ox + x) as usize] = 0;
            }
        }
    }
    gray
}

#[test]
fn test_extract_device_id_from_qr_payloads() {
    let id = sample_device_id();
    assert_eq!(extract_device_id(&id).as_deref(), Some(id.as_str()));
    assert_eq!(
        extract_device_id(&format!("  {}\n", id.to_lowercase())).as_deref(),
        Some(id.as_str())
    );
    assert_eq!(
        extract_device_id(&format!("syncthing://device/{}?name=pc", id)).as_deref(),
        Some(id.as_str())
    );
    // Short IDs, random text and URLs without an ID are not accepted from a QR code
    assert_eq!(extract_device_id(&id[..7]), None);
    assert_eq!(extract_device_id("https://syncthing.net/"), None);
    assert_eq!(extract_device_id(""), None);
}

#[test]
fn test_qr_decoder_reads_rendered_device_id() {
    let id = sample_device_id();
    let frame = render_qr_frame(&id);
    let payloads = QrDecoder::new().decode(&frame, CAMERA_W, CAMERA_H);
    assert_eq!(payloads, vec![id]);

    // Blank frame and short buffers decode to nothing
    let mut decoder = QrDecoder::new();
    assert!(decoder
        .decode(&vec![255u8; CAMERA_W * CAMERA_H], CAMERA_W, CAMERA_H)
        .is_empty());
    assert!(decoder.decode(&[0u8; 16], CAMERA_W, CAMERA_H).is_empty());
}

#[test]
fn test_scan_modal_opens_from_add_device_and_goes_back() {
    let dir = temp_test_dir("qr-modal");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 0,
    };
    assert_eq!(press(&mut state, &repo, BTN_R), UserCommand::None);
    assert_eq!(
        state.modal,
        ModalDialog::ScanDeviceQr {
            return_share_idx: None
        }
    );
    assert!(state.is_scanning_qr());

    // Camera errors are shown in the modal instead of closing it
    QrScanFlow::handle_event(
        &mut state,
        &repo,
        QrScanEvent::CameraError("No camera".to_owned()),
    );
    assert_eq!(state.qr_camera_error.as_deref(), Some("No camera"));
    assert!(state.is_scanning_qr());

    press(&mut state, &repo, BTN_CIRCLE);
    assert!(matches!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: None,
            ..
        }
    ));
}

#[test]
fn test_scanned_device_id_adds_device_and_starts_sync() {
    let dir = temp_test_dir("qr-add");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    QrScanFlow::open(&mut state, None);

    // A QR code that is not a Device ID keeps the scanner open
    let cmd = QrScanFlow::handle_event(
        &mut state,
        &repo,
        QrScanEvent::Payload("hello world".to_owned()),
    );
    assert_eq!(cmd, UserCommand::None);
    assert!(state.is_scanning_qr());
    assert!(state.devices.is_empty());

    let id = sample_device_id();
    let cmd = QrScanFlow::handle_event(&mut state, &repo, QrScanEvent::Payload(id.clone()));
    assert_eq!(cmd, UserCommand::StartSync);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.devices.len(), 1);
    assert_eq!(state.devices[0].device_id.to_syncthing_string(), id);
    assert_eq!(repo.load_devices().len(), 1);
}

#[test]
fn test_scanning_own_device_id_returns_to_form() {
    let dir = temp_test_dir("qr-own");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let id = sample_device_id();
    state.vita_device_id = id.clone();
    QrScanFlow::open(&mut state, Some(0));

    QrScanFlow::handle_event(&mut state, &repo, QrScanEvent::Payload(id.clone()));
    assert!(state.devices.is_empty());
    assert_eq!(state.device_id_input, id);
    assert!(matches!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: Some(0),
            ..
        }
    ));
}
