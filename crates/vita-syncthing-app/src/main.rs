//! PS Vita executable (`vita-syncthing-app`) — Clean Architecture Composition Root.
//!
//! The layers (`domain`, `application`, `infrastructure`, `ui`) live in the crate's library
//! (`lib.rs`); this file only wires them together and runs the event loop, which wakes up on every
//! display VBlank (60 Hz) but only renders a frame when needed (`FramePacer`).

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use syncthing_core::SyncControl;
use vita_syncthing_app::application::power_policy::{cpu_clock, should_block_standby, CpuClock};
use vita_syncthing_app::application::{
    FolderOfferService, FramePacer, FrameSignals, InputController, QrScanFlow, ShareService,
    SyncCoordinator, UserCommand,
};
use vita_syncthing_app::domain::{AppState, ModalDialog, UiText};
use vita_syncthing_app::infrastructure::{
    detect_system_language, init_vita_network, is_on_external_power, keep_system_awake,
    set_cpu_clock_mhz, spawn_sync_worker, wait_for_worker, ConfigRepository, IdentityStore,
    QrScanner, VitaInput, WorkerMsg,
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

/// How long quitting waits for the sync worker to stop cleanly.
const QUIT_WORKER_WAIT: Duration = Duration::from_secs(3);

fn main() {
    let identity_res = IdentityStore::load_or_create(BASE_DATA_DIR);
    let repo = ConfigRepository::new(BASE_DATA_DIR);

    let devices = repo.load_devices();
    let shares = repo.load_shares();
    let mut state = AppState::new(shares, devices, ConfigRepository::default_browse_root());
    state.theme_mode = repo.load_theme();
    state.set_system_language(detect_system_language());
    state.set_language_preference(repo.load_language());
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
    renderer.render(&mut state, 1);

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
        state.peer_label = UiText::NoPeerLinked;
        state.status_line = UiText::SelectDeviceToConnect;
    }

    let mut last_activity = Instant::now();
    let mut last_sync_finished = Instant::now();
    let mut last_power_tick = Instant::now() - Duration::from_secs(60);
    let mut seen_transfer_activity = state.transfer_activity;
    let mut last_transfer_at: Option<Instant> = None;
    let mut pacer = FramePacer::new();
    let mut cpu = CpuClock::Full;

    // Main interactive loop (Gamepad SceCtrl + Touch SceTouch + OLED Saver), one iteration per
    // display VBlank. Frames are only rendered when the screen changes (dynamic frame rate).
    loop {
        let mut signals = FrameSignals::default();
        let frame_input = input_ctl.poll();
        if frame_input.any_activity {
            last_activity = Instant::now();
        }
        signals.user_change |= frame_input.changes_view();
        let modal_before = state.modal;
        let saver_before = state.oled_saver_active;

        let mut command = InputController::handle_input(&mut state, &repo, frame_input);

        // Rear camera runs only while the QR scanner modal is open
        qr_scanner.set_active(state.is_scanning_qr());
        if state.is_scanning_qr() {
            last_activity = Instant::now();
            signals.animating = true;
            if let Some(event) = qr_scanner.poll(&mut state.qr_preview) {
                signals.user_change = true;
                let scan_cmd = QrScanFlow::handle_event(&mut state, &repo, event);
                if scan_cmd != UserCommand::None {
                    command = scan_cmd;
                }
            }
        }

        signals.user_change |= command != UserCommand::None;
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
                    state.initial_sync.clear();
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
                // Let the sessions close and save their state before the process exits.
                wait_for_worker(QUIT_WORKER_WAIT);
                set_cpu_clock_mhz(CpuClock::Full.mhz());
                break;
            }
        }
        let ime_open = input_ctl.is_ime_open();
        signals.user_change |= ime_open != state.ime_active;
        state.ime_active = ime_open;

        // Reload persistent worker if share/device configuration was changed and modals are closed
        if state.config_dirty && state.modal == ModalDialog::None {
            signals.background_change = true;
            state.config_dirty = false;
            if let Some(ref ctl) = worker_ctl {
                ctl.request_stop();
            }
            worker_rx = None;
            worker_ctl = None;
            state.session_connected = false;
            state.initial_sync.clear();
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
        signals.background_change |=
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
            signals.background_change = true;
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
        // Full CPU speed only during the initial sync phase and transfers (see `power_policy::cpu_clock`).
        let cpu_target = cpu_clock(
            state.initial_sync.is_active(),
            last_transfer_at.map(|t| t.elapsed()),
            last_activity.elapsed(),
            state.is_scanning_qr(),
        );
        if cpu_target != cpu {
            cpu = cpu_target;
            set_cpu_clock_mhz(cpu.mhz());
        }

        if last_power_tick.elapsed() >= Duration::from_secs(10) {
            last_power_tick = Instant::now();
            // The system may reset the clock after standby: re-apply it periodically.
            set_cpu_clock_mhz(cpu.mhz());
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

        // Toast lifetime is counted in VBlanks, whether or not frames are rendered.
        signals.user_change |= state.tick_toast();
        signals.user_change |= state.modal != modal_before;
        signals.user_change |= state.oled_saver_active != saver_before;
        signals.animating |= state.oled_saver_active;
        #[cfg(feature = "ui-script")]
        {
            signals.user_change |= vita_syncthing_app::infrastructure::ui_script::shot_pending();
        }

        // On PS Vita, both `render` (double-buffered flip) and `idle` block on the next VBlank,
        // which paces the loop at 60 iterations per second.
        match pacer.tick(signals) {
            Some(elapsed_vblanks) => renderer.render(&mut state, elapsed_vblanks),
            None => renderer.idle(),
        }
        #[cfg(not(target_os = "vita"))]
        std::thread::sleep(Duration::from_millis(16));
    }
}
