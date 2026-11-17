//! QR code device pairing: payload parsing, `quircs` decoding of a rendered QR code, scanner
//! modal navigation and adding the scanned device.

mod common;

use common::{press, rects_of, tap_rect, temp_test_dir};
use qrcodegen::{QrCode, QrCodeEcc};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{extract_device_id, QrScanFlow, UserCommand};
use vita_syncthing_app::domain::{AppState, ModalDialog, PadKey, UiAction, UiText};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, QrDecoder, QrScanEvent, BTN_CIRCLE, BTN_R, CAMERA_H, CAMERA_W,
};

fn sample_device_id() -> String {
    DeviceId([7u8; 32]).to_syncthing_string()
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

#[test]
fn test_scan_qr_action_in_manage_devices_modal() {
    let dir = temp_test_dir("qr-share");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let manage = ModalDialog::ManageDevices {
        share_idx: 0,
        selected_dev_idx: 0,
    };
    let scanning_share_0 = ModalDialog::ScanDeviceQr {
        return_share_idx: Some(0),
    };

    // `R` shortcut
    state.modal = manage;
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.modal, scanning_share_0);

    // `R · Scan QR` button at the bottom of the modal, and its hint in the bottom bar
    state.modal = manage;
    let scan = rects_of(&mut state, UiAction::Press(PadKey::R));
    assert_eq!(scan.len(), 2, "the dialog button and the hint");
    for rect in scan {
        state.modal = manage;
        tap_rect(&mut state, &repo, rect);
        assert_eq!(state.modal, scanning_share_0);
    }

    // `+ Add by ID` button next to it still opens the ID form
    state.modal = manage;
    let add = rects_of(&mut state, UiAction::Press(PadKey::Square))[0];
    tap_rect(&mut state, &repo, add);
    assert!(matches!(state.modal, ModalDialog::AddDeviceById { .. }));
}

#[test]
fn test_scan_qr_action_in_show_device_id_modal() {
    let dir = temp_test_dir("qr-show-id");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    let scanning = ModalDialog::ScanDeviceQr {
        return_share_idx: None,
    };

    // `R` shortcut
    state.modal = ModalDialog::ShowDeviceId;
    press(&mut state, &repo, BTN_R);
    assert_eq!(state.modal, scanning);

    // `R · Scan QR` button next to `Add Remote Device`, and its hint in the bottom bar
    state.modal = ModalDialog::ShowDeviceId;
    let scan = rects_of(&mut state, UiAction::Press(PadKey::R));
    assert_eq!(scan.len(), 2, "the dialog button and the hint");
    for rect in scan {
        state.modal = ModalDialog::ShowDeviceId;
        tap_rect(&mut state, &repo, rect);
        assert_eq!(state.modal, scanning);
    }

    // `Add Remote Device` still opens the ID form
    state.modal = ModalDialog::ShowDeviceId;
    let add = rects_of(&mut state, UiAction::Press(PadKey::Square))[0];
    tap_rect(&mut state, &repo, add);
    assert!(matches!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: None,
            ..
        }
    ));
}

#[test]
fn test_extract_device_id_finds_the_first_id_inside_longer_text() {
    let id = sample_device_id();
    let other = DeviceId([9u8; 32]).to_syncthing_string();
    assert_eq!(
        extract_device_id(&format!("My device is {} thanks", id)).as_deref(),
        Some(id.as_str())
    );
    assert_eq!(
        extract_device_id(&format!("{} then {}", id, other)).as_deref(),
        Some(id.as_str()),
        "the first valid ID wins"
    );
    // A token with one wrong character fails the Luhn check and is rejected.
    let mut bad = id.clone();
    let first = bad[..1].to_owned();
    bad.replace_range(0..1, if first == "A" { "B" } else { "A" });
    assert_eq!(extract_device_id(&bad), None);
}

#[test]
fn test_scanner_open_and_close_keep_the_return_target() {
    let dir = temp_test_dir("qr-open-close");
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    state.ime_active = true;
    state.qr_preview.clear();
    state.qr_camera_error = Some("old error".to_owned());

    QrScanFlow::open(&mut state, Some(2));
    assert!(!state.ime_active);
    assert_eq!(state.qr_camera_error, None);
    assert_eq!(
        state.modal,
        ModalDialog::ScanDeviceQr {
            return_share_idx: Some(2)
        }
    );
    assert!(state.is_scanning_qr());

    QrScanFlow::close(&mut state, Some(2));
    assert_eq!(
        state.modal,
        ModalDialog::AddDeviceById {
            return_share_idx: Some(2),
            keypad_idx: 0
        }
    );
    assert!(!state.is_scanning_qr());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_scanner_events_outside_the_scanner_are_ignored_and_errors_are_kept() {
    use vita_syncthing_app::infrastructure::QrScanEvent as Event;
    let dir = temp_test_dir("qr-events");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // Not in the scanner: no command, nothing changes.
    let cmd = QrScanFlow::handle_event(&mut state, &repo, Event::Payload(sample_device_id()));
    assert_eq!(cmd, UserCommand::None);
    assert!(state.device_id_input.is_empty());

    QrScanFlow::open(&mut state, None);
    assert_eq!(
        QrScanFlow::handle_event(
            &mut state,
            &repo,
            Event::CameraError("no camera".to_owned())
        ),
        UserCommand::None
    );
    assert_eq!(state.qr_camera_error.as_deref(), Some("no camera"));
    assert!(state.is_scanning_qr());

    // A QR code that is not a device ID stays in the scanner with a toast.
    QrScanFlow::handle_event(&mut state, &repo, Event::Payload("hello".to_owned()));
    assert!(state.is_scanning_qr());
    assert_eq!(
        state.toast_msg.as_ref().map(|(m, _)| m.clone()),
        Some(UiText::QrNotDeviceId)
    );
    assert!(state.device_id_input.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
