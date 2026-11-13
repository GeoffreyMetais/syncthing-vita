//! `DevicesScan`: LAN scan of the `ManageDevices` dialog — immediate for a share with no linked
//! device, after `DEVICES_DIALOG_SCAN_DELAY` otherwise (cancelled when the dialog closes), also
//! while another device is connected.

mod common;

use common::{press, tap_action, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DevicesScan, ShareService, UserCommand};
use vita_syncthing_app::domain::defaults::DEVICES_DIALOG_SCAN_DELAY;
use vita_syncthing_app::domain::{AppState, ModalDialog, UiAction};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_CIRCLE};

const PEER: DeviceId = DeviceId([0x42; 32]);

/// One share `saves` (linked to `PEER` when `linked`), `PEER` known, no dialog open, idle.
fn setup(name: &str, linked: bool) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    if linked {
        ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER);
    }
    state.modal = ModalDialog::None;
    state.sync_in_progress = false;
    state.session_connected = false;
    (dir, repo, state)
}

/// Runs `vblanks` main-loop ticks; returns the VBlank (1-based) at which a scan started.
fn run_ticks(state: &mut AppState, vblanks: u32) -> Option<u32> {
    (1..=vblanks).find(|_| DevicesScan::tick(state) == UserCommand::ScanLan)
}

#[test]
fn test_share_without_linked_device_scans_on_opening() {
    let (dir, repo, mut state) = setup("devices-scan-immediate", false);
    assert_eq!(
        tap_action(&mut state, &repo, UiAction::ManageDevices(0)),
        UserCommand::ScanLan
    );
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));
    // Once per opening: nothing more while the dialog stays open.
    assert_eq!(press(&mut state, &repo, 0), UserCommand::None);
    assert_eq!(run_ticks(&mut state, 2 * DEVICES_DIALOG_SCAN_DELAY), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_share_with_linked_device_scans_after_the_delay() {
    let (dir, repo, mut state) = setup("devices-scan-delayed", true);
    assert_eq!(
        tap_action(&mut state, &repo, UiAction::ManageDevices(0)),
        UserCommand::None
    );
    assert_eq!(
        run_ticks(&mut state, 2 * DEVICES_DIALOG_SCAN_DELAY),
        Some(DEVICES_DIALOG_SCAN_DELAY)
    );
    assert_eq!(state.devices_scan_countdown, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_closing_the_dialog_cancels_the_delayed_scan() {
    let (dir, repo, mut state) = setup("devices-scan-cancelled", true);
    tap_action(&mut state, &repo, UiAction::ManageDevices(0));
    assert_eq!(run_ticks(&mut state, DEVICES_DIALOG_SCAN_DELAY / 2), None);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.devices_scan_countdown, None);
    assert_eq!(run_ticks(&mut state, 2 * DEVICES_DIALOG_SCAN_DELAY), None);

    // Reopening starts a full delay again.
    tap_action(&mut state, &repo, UiAction::ManageDevices(0));
    assert_eq!(
        run_ticks(&mut state, 2 * DEVICES_DIALOG_SCAN_DELAY),
        Some(DEVICES_DIALOG_SCAN_DELAY)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_scan_also_runs_while_another_device_is_connected() {
    for linked in [false, true] {
        let (dir, repo, mut state) = setup("devices-scan-connected", linked);
        state.sync_in_progress = true;
        state.session_connected = true;
        let opened = tap_action(&mut state, &repo, UiAction::ManageDevices(0));
        let scanned = if linked {
            assert_eq!(opened, UserCommand::None);
            run_ticks(&mut state, 2 * DEVICES_DIALOG_SCAN_DELAY)
        } else {
            assert_eq!(opened, UserCommand::ScanLan);
            Some(0)
        };
        let expected = if linked { DEVICES_DIALOG_SCAN_DELAY } else { 0 };
        assert_eq!(scanned, Some(expected), "linked: {linked}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
