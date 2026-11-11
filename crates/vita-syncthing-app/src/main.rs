//! PS Vita executable (`vita-syncthing-app`) — Clean Architecture Composition Root.
//!
//! The layers (`domain`, `application`, `infrastructure`, `ui`) live in the crate's library
//! (`lib.rs`); this file only wires them together and runs the 60 FPS event loop.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use syncthing_core::SyncControl;
use vita_syncthing_app::application::power_policy::should_block_standby;
use vita_syncthing_app::application::{
    FolderOfferService, InputController, QrScanFlow, ShareService, SyncCoordinator, UserCommand,
};
use vita_syncthing_app::domain::{AppState, ModalDialog};
use vita_syncthing_app::infrastructure::{
    init_vita_network, is_on_external_power, keep_system_awake, spawn_sync_worker,
    ConfigRepository, IdentityStore, QrScanner, VitaInput, WorkerMsg,
};
use vita_syncthing_app::ui::VitaUiRenderer;

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
    state.ignored_folders = repo.load_ignored_folders();
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
    let mut qr_scanner = QrScanner::new();

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
    let mut seen_transfer_activity = state.transfer_activity;
    let mut last_transfer_at: Option<Instant> = None;

    // Main interactive 60 FPS loop (Gamepad SceCtrl + Touch SceTouch + OLED Saver)
    loop {
        let frame_input = input_ctl.poll();
        if frame_input.any_activity {
            last_activity = Instant::now();
        }

        let mut command = InputController::handle_input(&mut state, &repo, frame_input);

        // Rear camera runs only while the QR scanner modal is open
        qr_scanner.set_active(state.is_scanning_qr());
        if state.is_scanning_qr() {
            last_activity = Instant::now();
            if let Some(event) = qr_scanner.poll(&mut state.qr_preview) {
                let scan_cmd = QrScanFlow::handle_event(&mut state, &repo, event);
                if scan_cmd != UserCommand::None {
                    command = scan_cmd;
                }
            }
        }

        match command {
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
        FolderOfferService::prompt_next(&mut state);
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

        // Keep the console out of automatic standby only when useful: while charging, or during
        // transfers (plus a grace period). On battery and idle, standby is allowed.
        if state.transfer_activity != seen_transfer_activity {
            seen_transfer_activity = state.transfer_activity;
            last_transfer_at = Some(Instant::now());
        }
        if last_power_tick.elapsed() >= Duration::from_secs(10) {
            last_power_tick = Instant::now();
            let sync_active = worker_rx.is_some() || state.auto_reconnect_enabled;
            if should_block_standby(
                sync_active,
                is_on_external_power(),
                last_transfer_at.map(|t| t.elapsed()),
            ) {
                keep_system_awake();
            }
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
