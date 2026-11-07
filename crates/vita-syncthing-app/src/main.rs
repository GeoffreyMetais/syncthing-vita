//! PS Vita executable (`vita-syncthing-app`) — Clean Architecture Composition Root.
//!
//! Layers:
//! - `domain`: Pure domain models (`KnownDevice`, `ShareConfigEntry`, `PickerItem`) and `AppState`.
//! - `application`: Use cases (`ShareService`), `SyncCoordinator`, and `InputController`.
//! - `infrastructure`: `ConfigRepository`, `IdentityStore`, `VitaFramebuffer`, `VitaInput`,
//!   `init_vita_network`, and background `spawn_sync_worker`.
//! - `ui`: Pure presentation & rendering layer (`VitaUiRenderer`, `screens`, `modals`, `widgets`, `theme`, `gfx`).

mod application;
mod domain;
mod infrastructure;
mod ui;

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use application::{InputController, ShareService, SyncCoordinator, UserCommand};
use domain::AppState;
use infrastructure::{
    init_vita_network, spawn_sync_worker, ConfigRepository, IdentityStore, VitaInput, WorkerMsg,
};
use ui::VitaUiRenderer;

/// Explicit 64 MB Newlib heap size on PS Vita hardware.
/// Without `_newlib_heap_size_user`, VitaSDK's `_init_vita_heap` attempts a single 128 MB
/// `sceKernelAllocMemBlock` which fails on real PS Vita hardware (`_newlib_heap_base = NULL`),
/// causing the very first Rust `malloc` to abort with `C2-12828-1`.
#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static _newlib_heap_size_user: u32 = 64 * 1024 * 1024;

#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static sceUserMainThreadStackSize: u32 = 2 * 1024 * 1024;

#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static sceUserMainThreadName: [u8; 16] = *b"vita_syncthing\0\0";

#[cfg(target_os = "vita")]
const BASE_DATA_DIR: &str = "ux0:data/syncthing";
#[cfg(not(target_os = "vita"))]
const BASE_DATA_DIR: &str = "/tmp/syncthing-vita";

fn main() {
    let identity_res = IdentityStore::load_or_create(BASE_DATA_DIR);
    let repo = ConfigRepository::new(BASE_DATA_DIR);

    let devices = repo.load_devices();
    let shares = repo.load_shares();
    let mut state = AppState::new(shares, devices, ConfigRepository::default_browse_root());
    ShareService::refresh_picker_items(&mut state, &repo);

    let mut renderer = Box::new(VitaUiRenderer::new());
    renderer.render(&mut state);

    let mut input_ctl = VitaInput::new();

    ShareService::log_event(
        &mut state,
        &repo,
        "=== VitaSyncthing // Gamepad + Touch UI + Share Manager + OLED Saver ===",
    );

    let _net_pool = match init_vita_network() {
        Ok(pool) => {
            ShareService::log_event(&mut state, &repo, "sceNet network subsystem ready.");
            pool
        }
        Err(e) => {
            ShareService::log_event(&mut state, &repo, &format!("Network init error: {}", e));
            return;
        }
    };

    let identity = match identity_res {
        Ok(id) => id,
        Err(e) => {
            ShareService::log_event(&mut state, &repo, &format!("X.509 identity error: {}", e));
            return;
        }
    };

    state.vita_device_id = identity.device_id.to_syncthing_string();
    ShareService::log_event(
        &mut state,
        &repo,
        &format!("PS Vita Device ID: {}", identity.device_id),
    );

    // Automatically trigger initial synchronization on startup
    state.sync_in_progress = true;
    let mut worker_rx: Option<Receiver<WorkerMsg>> = Some(spawn_sync_worker(
        repo.base_data_dir(),
        identity.clone(),
        state.shares.clone(),
        state.devices.clone(),
    ));

    let mut last_activity = Instant::now();
    let mut last_sync_finished = Instant::now();

    // Main interactive 60 FPS loop (Gamepad SceCtrl + Touch SceTouch + OLED Saver)
    loop {
        let frame_input = input_ctl.poll();
        if frame_input.any_activity {
            last_activity = Instant::now();
        }

        match InputController::handle_input(&mut state, &repo, frame_input) {
            UserCommand::None => {}
            UserCommand::StartSync => {
                if !state.sync_in_progress {
                    state.prepare_manual_sync();
                    ShareService::log_event(
                        &mut state,
                        &repo,
                        "Starting BEP/1.0 synchronization...",
                    );
                    worker_rx = Some(spawn_sync_worker(
                        repo.base_data_dir(),
                        identity.clone(),
                        state.shares.clone(),
                        state.devices.clone(),
                    ));
                }
            }
            UserCommand::QuitApp => {
                ShareService::log_event(&mut state, &repo, "Exit requested by user [START].");
                break;
            }
        }

        let was_running = worker_rx.is_some();
        SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
        if was_running && worker_rx.is_none() {
            last_sync_finished = Instant::now();
        }

        // Automatically maintain BEP/1.0 connection in the background every 1s when idle
        if worker_rx.is_none() && last_sync_finished.elapsed() >= Duration::from_secs(1) {
            state.sync_in_progress = true;
            worker_rx = Some(spawn_sync_worker(
                repo.base_data_dir(),
                identity.clone(),
                state.shares.clone(),
                state.devices.clone(),
            ));
        }

        // Automatic activation of OLED screensaver (pure black #000000 background):
        // - After 12 seconds of inactivity while synchronization is in progress
        // - After 45 seconds of inactivity when idle (or press SELECT anytime)
        let oled_timeout = if state.sync_in_progress {
            Duration::from_secs(12)
        } else {
            Duration::from_secs(45)
        };
        if !state.oled_saver_active && last_activity.elapsed() >= oled_timeout {
            state.oled_saver_active = true;
            repo.write_log("Automatic OLED screensaver activation (#000000).");
        }

        renderer.render(&mut state);
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActiveTab, ModalDialog, ThemeMode};
    use crate::infrastructure::{InputFrame, BTN_CROSS, BTN_START};
    use syncthing_core::DeviceId;

    fn temp_test_dir(suffix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = format!("/tmp/vita-syncthing-test-{}-{}", suffix, nanos);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_share_service_and_config_repository_roundtrip() {
        let dir = temp_test_dir("share-service");
        let repo = ConfigRepository::new(&dir);
        let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

        let dev_id = DeviceId([0x42; 32]);
        ShareService::upsert_device(&mut state, &repo, dev_id, "MacBook", "192.168.1.10:22000");
        assert_eq!(state.devices.len(), 1);

        let idx = ShareService::create_share(
            &mut state,
            &repo,
            "vita-savedata",
            "Vita Save Data",
            "ux0:user/00/savedata",
        );
        assert_eq!(idx, 0);
        assert_eq!(state.shares.len(), 1);
        assert!(state.shares[0].enabled);
        assert_eq!(state.shares[0].shared_devices, vec![dev_id.to_hex()]);

        // Toggle device off and back on
        let (_, _, added) =
            ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
        assert!(!added);
        assert!(state.shares[0].shared_devices.is_empty());

        let (_, _, added_again) =
            ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
        assert!(added_again);
        assert_eq!(state.shares[0].shared_devices, vec![dev_id.to_hex()]);

        // Discovered remote folder starts paused (enabled = false)
        ShareService::upsert_discovered_folder(&mut state, &repo, "remote-docs", "Remote Docs");
        assert_eq!(state.shares.len(), 2);
        assert!(!state.shares[1].enabled);

        // Reload from disk via ConfigRepository
        let loaded_devs = repo.load_devices();
        let loaded_shares = repo.load_shares();
        assert_eq!(loaded_devs, state.devices);
        assert_eq!(loaded_shares, state.shares);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_input_controller_quit_dialog_and_theme_toggle() {
        let dir = temp_test_dir("input-ctl");
        let repo = ConfigRepository::new(&dir);
        let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

        // Pressing START opens ConfirmQuit modal instead of quitting immediately
        let cmd = InputController::handle_input(
            &mut state,
            &repo,
            InputFrame {
                pressed: BTN_START,
                touch_tap: None,
                touch_pos: None,
                any_activity: true,
            },
        );
        assert_eq!(cmd, UserCommand::None);
        assert_eq!(state.modal, ModalDialog::ConfirmQuit);

        // Pressing CROSS inside ConfirmQuit returns UserCommand::QuitApp
        let cmd2 = InputController::handle_input(
            &mut state,
            &repo,
            InputFrame {
                pressed: BTN_CROSS,
                touch_tap: None,
                touch_pos: None,
                any_activity: true,
            },
        );
        assert_eq!(cmd2, UserCommand::QuitApp);

        // Switching to Settings and pressing CROSS toggles Dark -> OLED theme
        state.modal = ModalDialog::None;
        state.active_tab = ActiveTab::Settings;
        assert_eq!(state.theme_mode, ThemeMode::Dark);
        let _ = InputController::handle_input(
            &mut state,
            &repo,
            InputFrame {
                pressed: BTN_CROSS,
                touch_tap: None,
                touch_pos: None,
                any_activity: true,
            },
        );
        assert_eq!(state.theme_mode, ThemeMode::Oled);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
