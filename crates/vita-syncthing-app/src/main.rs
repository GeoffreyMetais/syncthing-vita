//! PS Vita executable (`vita-syncthing-app`) — Clean Architecture Composition Root.
//!
//! The layers (`domain`, `application`, `infrastructure`, `ui`) live in the crate's library
//! (`lib.rs`); this file only wires them together and runs the event loop, which wakes up on every
//! display VBlank (60 Hz) but only renders a frame when needed (`FramePacer`).

#[cfg(not(target_os = "vita"))]
use std::time::Duration;
use std::time::Instant;

use vita_syncthing_app::application::power_policy::{cpu_clock, should_block_standby, CpuClock};
use vita_syncthing_app::application::{
    DevicesScan, FolderBrowser, FramePacer, FrameSignals, InputController, InputGestures,
    QrScanFlow, ShareService, SyncSupervisor, UploadActivity, UserCommand,
};
use vita_syncthing_app::domain::defaults::{
    POWER_TICK_INTERVAL, SCREENSAVER_TIMEOUT_IDLE, SCREENSAVER_TIMEOUT_SYNCING,
};
use vita_syncthing_app::domain::AppState;
use vita_syncthing_app::infrastructure::{
    detect_system_language, init_vita_network, is_on_external_power, keep_system_awake,
    set_cpu_clock_mhz, ConfigRepository, IdentityStore, QrScanner, VitaInput,
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

/// Host builds have no VBlank to wait for: sleep one 60 Hz frame per loop iteration.
#[cfg(not(target_os = "vita"))]
const HOST_FRAME_SLEEP: Duration = Duration::from_millis(16);

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
    state.ignored_devices = repo.load_ignored_devices();
    if state.sanitize_legacy_unpaired_peers() {
        repo.save_devices(&state.approved_devices());
        repo.save_shares(&state.shares);
    }
    if let Ok(ref id) = identity_res {
        state.vita_device_id = id.device_id.to_syncthing_string();
    }
    FolderBrowser::refresh(&mut state, &repo);

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

    // Auto-connect on startup if at least one peer has an active share (or no share exists yet)
    let mut sync = SyncSupervisor::new(identity);
    sync.start_at_launch(&mut state, &repo);

    let mut last_activity = Instant::now();
    // Apply the power policy on the first iteration.
    let mut last_power_tick = Instant::now() - POWER_TICK_INTERVAL;
    let mut seen_transfer_activity = state.transfer_activity;
    let mut last_transfer_at: Option<Instant> = None;
    let mut uploads = UploadActivity::new();
    let mut pacer = FramePacer::new();
    let mut gestures = InputGestures::new();
    let mut cpu = CpuClock::Full;

    // Main interactive loop (Gamepad SceCtrl + Touch SceTouch + OLED Saver), one iteration per
    // display VBlank. Frames are only rendered when the screen changes (dynamic frame rate).
    loop {
        let mut signals = FrameSignals::default();
        let mut frame_input = input_ctl.poll();
        gestures.apply(&mut frame_input);
        if frame_input.any_activity {
            last_activity = Instant::now();
        }
        signals.user_change |= frame_input.changes_view();
        let modal_before = state.modal;
        let saver_before = state.oled_saver_active;

        let mut command = InputController::handle_input(&mut state, &repo, frame_input);
        // Delayed LAN scan of the devices dialog, counted in VBlanks (one per loop iteration).
        let scan_cmd = DevicesScan::tick(&mut state);
        if command == UserCommand::None {
            command = scan_cmd;
        }

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

        // A LAN scan goes to the running worker; without one, a sync starts with a discovery.
        if command == UserCommand::ScanLan {
            command = if sync.request_discovery() {
                UserCommand::None
            } else {
                UserCommand::StartSync
            };
        }
        signals.user_change |= command != UserCommand::None;
        match command {
            UserCommand::None | UserCommand::ScanLan => {}
            UserCommand::OpenSystemKeyboard => {
                state.ime_active = input_ctl.open_ime(&state.device_id_input);
            }
            UserCommand::StartSync => {
                input_ctl.close_ime();
                state.ime_active = false;
                sync.start_sync(&mut state, &repo);
            }
            UserCommand::QuitApp => {
                input_ctl.close_ime();
                ShareService::log_event(&mut state, &repo, "Exit requested by user [START].");
                sync.shutdown();
                set_cpu_clock_mhz(CpuClock::Full.mhz());
                break;
            }
        }
        let ime_open = input_ctl.is_ime_open();
        signals.user_change |= ime_open != state.ime_active;
        state.ime_active = ime_open;

        // Sync worker life cycle: configuration reloads, worker messages, reconnection.
        signals.background_change |= sync.tick(&mut state, &repo);
        signals.background_change |= uploads.update(&mut state, Instant::now());

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

        if last_power_tick.elapsed() >= POWER_TICK_INTERVAL {
            last_power_tick = Instant::now();
            // The system may reset the clock after standby: re-apply it periodically.
            set_cpu_clock_mhz(cpu.mhz());
            let sync_active = sync.is_running() || state.auto_reconnect_enabled;
            if should_block_standby(
                sync_active,
                is_on_external_power(),
                last_transfer_at.map(|t| t.elapsed()),
            ) {
                keep_system_awake();
            }
        }

        // Automatic activation of OLED screensaver (pure black #000000 background):
        // - After 10 seconds of inactivity while synchronization is in progress
        // - After 30 seconds of inactivity when idle
        let oled_timeout = if state.sync_in_progress {
            SCREENSAVER_TIMEOUT_SYNCING
        } else {
            SCREENSAVER_TIMEOUT_IDLE
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
        state.tick_progress_anim();
        signals.animating |= state.oled_saver_active || state.progress_pct.is_none();
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
        std::thread::sleep(HOST_FRAME_SLEEP);
    }
}
