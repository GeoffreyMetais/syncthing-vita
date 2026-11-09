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
use std::sync::Arc;
use std::time::{Duration, Instant};

use application::{InputController, ShareService, SyncCoordinator, UserCommand};
use domain::{AppState, ModalDialog};
use infrastructure::{
    init_vita_network, keep_system_awake, spawn_sync_worker, ConfigRepository, IdentityStore,
    VitaInput, WorkerMsg,
};
use syncthing_core::SyncControl;
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
    if state.sanitize_legacy_unpaired_peers() {
        repo.save_devices(&state.approved_devices());
        repo.save_shares(&state.shares);
    }
    if let Ok(ref id) = identity_res {
        state.vita_device_id = id.device_id.to_syncthing_string();
    }
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

    let mut worker_rx: Option<Receiver<WorkerMsg>> = None;
    let mut worker_ctl: Option<Arc<SyncControl>> = None;

    // Only auto-connect on startup if the user has already configured/paired at least one device
    if state.has_configured_peers() {
        state.sync_in_progress = true;
        let offered = std::mem::take(&mut state.pending_folder_offers);
        let (rx, ctl) = spawn_sync_worker(
            repo.base_data_dir(),
            identity.clone(),
            state.shares.clone(),
            state.approved_devices(),
            offered,
        );
        worker_rx = Some(rx);
        worker_ctl = Some(ctl);
    } else {
        state.peer_label = "No peer linked (Manual pairing)".to_owned();
        state.status_line = "Select or add a device in Shares -> Devices to connect".to_owned();
    }

    let mut last_activity = Instant::now();
    let mut last_sync_finished = Instant::now();
    let mut last_power_tick = Instant::now() - Duration::from_secs(60);

    // Main interactive 60 FPS loop (Gamepad SceCtrl + Touch SceTouch + OLED Saver)
    loop {
        let frame_input = input_ctl.poll();
        if frame_input.any_activity {
            last_activity = Instant::now();
        }

        match InputController::handle_input(&mut state, &repo, frame_input) {
            UserCommand::None => {}
            UserCommand::OpenSystemKeyboard => {
                state.ime_active = input_ctl.open_ime(&state.device_id_input);
            }
            UserCommand::StartSync => {
                input_ctl.close_ime();
                state.ime_active = false;
                state.auto_reconnect_enabled = true;
                if worker_rx.is_some() && state.session_connected && !state.config_dirty {
                    state.prepare_manual_sync();
                    ShareService::log_event(
                        &mut state,
                        &repo,
                        "Triggering live local rescan over persistent 1+2 connection...",
                    );
                    if let Some(ref ctl) = worker_ctl {
                        ctl.request_rescan();
                    }
                } else {
                    if let Some(ref ctl) = worker_ctl {
                        ctl.request_stop();
                    }
                    state.config_dirty = false;
                    state.session_connected = false;
                    state.prepare_manual_sync();
                    ShareService::log_event(
                        &mut state,
                        &repo,
                        "Starting BEP/1.0 synchronization...",
                    );
                    let offered = std::mem::take(&mut state.pending_folder_offers);
                    let (rx, ctl) = spawn_sync_worker(
                        repo.base_data_dir(),
                        identity.clone(),
                        state.shares.clone(),
                        state.approved_devices(),
                        offered,
                    );
                    worker_rx = Some(rx);
                    worker_ctl = Some(ctl);
                }
            }
            UserCommand::QuitApp => {
                input_ctl.close_ime();
                if let Some(ref ctl) = worker_ctl {
                    ctl.request_stop();
                }
                ShareService::log_event(&mut state, &repo, "Exit requested by user [START].");
                break;
            }
        }
        state.ime_active = input_ctl.is_ime_open();

        // Reload persistent worker if share/device configuration was changed and modals are closed
        if state.config_dirty && state.modal == ModalDialog::None {
            state.config_dirty = false;
            if let Some(ref ctl) = worker_ctl {
                ctl.request_stop();
            }
            worker_rx = None;
            worker_ctl = None;
            state.session_connected = false;
            if state.has_configured_peers() {
                state.sync_in_progress = true;
                let offered = std::mem::take(&mut state.pending_folder_offers);
                let (rx, ctl) = spawn_sync_worker(
                    repo.base_data_dir(),
                    identity.clone(),
                    state.shares.clone(),
                    state.approved_devices(),
                    offered,
                );
                worker_rx = Some(rx);
                worker_ctl = Some(ctl);
            }
        }

        let was_running = worker_rx.is_some();
        SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
        if was_running && worker_rx.is_none() {
            worker_ctl = None;
            last_sync_finished = Instant::now();
        }

        // Automatically reconnect to user-approved peers if the persistent session dropped
        if worker_rx.is_none()
            && state.auto_reconnect_enabled
            && state.has_configured_peers()
            && last_sync_finished.elapsed() >= Duration::from_secs(2)
        {
            state.sync_in_progress = true;
            let offered = std::mem::take(&mut state.pending_folder_offers);
            let (rx, ctl) = spawn_sync_worker(
                repo.base_data_dir(),
                identity.clone(),
                state.shares.clone(),
                state.approved_devices(),
                offered,
            );
            worker_rx = Some(rx);
            worker_ctl = Some(ctl);
        }

        // Keep the console out of automatic standby while connected / reconnecting to peers.
        if (worker_rx.is_some() || state.auto_reconnect_enabled)
            && last_power_tick.elapsed() >= Duration::from_secs(10)
        {
            last_power_tick = Instant::now();
            keep_system_awake();
        }

        // Automatic activation of OLED screensaver (pure black #000000 background):
        // - After 12 seconds of inactivity while synchronization is in progress
        // - After 45 seconds of inactivity when idle (or press SELECT anytime)
        let oled_timeout = if state.sync_in_progress {
            Duration::from_secs(12)
        } else {
            Duration::from_secs(45)
        };
        if !state.oled_saver_active && !state.ime_active && last_activity.elapsed() >= oled_timeout
        {
            state.oled_saver_active = true;
            repo.write_log("Automatic OLED screensaver activation (#000000).");
        }

        // On PS Vita, `render` blocks on VBlank (double-buffered flip), which paces the loop at 60 FPS.
        renderer.render(&mut state);
        #[cfg(not(target_os = "vita"))]
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
        assert!(state.shares[0].shared_devices.is_empty());
        assert!(state.pending_folder_offers.is_empty());

        // Link device explicitly to share
        let (_, _, added) =
            ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
        assert!(added);
        assert_eq!(state.shares[0].shared_devices, vec![dev_id.to_hex()]);
        assert_eq!(
            state.pending_folder_offers,
            vec!["vita-savedata".to_owned()]
        );

        // Toggle device off and back on
        let (_, _, added_off) =
            ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
        assert!(!added_off);
        assert!(state.shares[0].shared_devices.is_empty());

        let (_, _, added_again) =
            ShareService::toggle_device_for_share(&mut state, &repo, 0, dev_id).unwrap();
        assert!(added_again);
        assert_eq!(state.shares[0].shared_devices, vec![dev_id.to_hex()]);

        // Discovered remote folder starts paused (enabled = false)
        ShareService::upsert_discovered_folder(
            &mut state,
            &repo,
            dev_id,
            "remote-docs",
            "Remote Docs",
        );
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
                any_activity: true,
                ..Default::default()
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
                any_activity: true,
                ..Default::default()
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
                any_activity: true,
                ..Default::default()
            },
        );
        assert_eq!(state.theme_mode, ThemeMode::Oled);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_add_device_by_short_id_and_system_keyboard_flow() {
        use crate::infrastructure::BTN_SQUARE;

        let dir = temp_test_dir("add-device-ime");
        let repo = ConfigRepository::new(&dir);
        let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

        let share_idx = ShareService::create_share(
            &mut state,
            &repo,
            "vita-savedata",
            "Vita Save Data",
            "ux0:user/00/savedata",
        );
        state.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx: 0,
        };

        // 1. Pressing SQUARE in ManageDevices opens AddDeviceById and requests PS Vita System Keyboard
        let cmd = InputController::handle_input(
            &mut state,
            &repo,
            InputFrame {
                pressed: BTN_SQUARE,
                any_activity: true,
                ..Default::default()
            },
        );
        assert_eq!(cmd, UserCommand::OpenSystemKeyboard);
        assert_eq!(
            state.modal,
            ModalDialog::AddDeviceById {
                return_share_idx: Some(share_idx),
                keypad_idx: 0,
            }
        );

        // 2. Simulate entering a 7-char Short ID via PS Vita System Keyboard (`SceIme`) and pressing Enter
        let peer_seed = *b"REMOTE_PC_SYNCTHING_SEED_0000001";
        let full_peer_id = syncthing_core::VitaIdentity::from_seed(&peer_seed)
            .unwrap()
            .device_id;
        let short_id = full_peer_id.short();

        let cmd2 = InputController::handle_input(
            &mut state,
            &repo,
            InputFrame {
                ime_text: Some(short_id.to_lowercase()),
                ime_submitted: true,
                any_activity: true,
                ..Default::default()
            },
        );
        assert_eq!(cmd2, UserCommand::StartSync);
        assert_eq!(state.devices.len(), 1);
        assert!(state.devices[0].device_id.is_short_placeholder());
        assert_eq!(state.devices[0].device_id.short(), short_id);
        assert!(state.devices[0].device_id.matches_peer(&full_peer_id));
        assert_eq!(
            state.shares[share_idx].shared_devices,
            vec![state.devices[0].device_id.to_hex()]
        );

        // 3. When the peer is subsequently discovered on LAN, its Short ID placeholder automatically upgrades to the full 32-byte DeviceId
        ShareService::upsert_device(
            &mut state,
            &repo,
            full_peer_id,
            "Gaming-PC",
            "192.168.1.42:22000",
        );
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].device_id, full_peer_id);
        assert!(!state.devices[0].device_id.is_short_placeholder());
        assert_eq!(
            state.shares[share_idx].shared_devices,
            vec![full_peer_id.to_hex()]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_passive_lan_discovery_does_not_auto_link_or_connect() {
        let dir = temp_test_dir("manual-lan-pairing");
        let repo = ConfigRepository::new(&dir);
        let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

        let random_lan_peer = DeviceId([0x77; 32]);
        ShareService::register_discovered_peer(
            &mut state,
            &repo,
            random_lan_peer,
            "LAN Peer",
            "192.168.1.99:22000",
        );

        // Discovered LAN peer is visible in UI list (`state.devices`) but NOT approved for automatic connection
        assert_eq!(state.devices.len(), 1);
        assert!(!state.has_configured_peers());
        assert!(state.approved_devices().is_empty());

        // Creating a share must NOT auto-link unapproved LAN peers
        let share_idx = ShareService::create_share(
            &mut state,
            &repo,
            "vita-savedata",
            "Vita Save Data",
            "ux0:user/00/savedata",
        );
        assert!(state.shares[share_idx].shared_devices.is_empty());
        assert!(!state.has_configured_peers());

        // Only when the user manually ticks the peer in ManageDevices does it become approved
        let (_, _, added) =
            ShareService::toggle_device_for_share(&mut state, &repo, share_idx, random_lan_peer)
                .unwrap();
        assert!(added);
        assert!(state.has_configured_peers());
        assert_eq!(state.approved_devices().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
