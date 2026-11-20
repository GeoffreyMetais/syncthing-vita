//! Pins the tunable values of `domain/defaults.rs`: a change must be deliberate, and derived
//! constants (`3 * VBLANK_HZ`, `LOG_MAX_BYTES`) must keep their meaning.

use std::time::Duration;
use vita_syncthing_app::domain::defaults::*;

#[test]
fn test_device_identity_names() {
    assert_eq!(DEVICE_NAME, "PS-Vita");
    assert_eq!(CLIENT_NAME, "vita-syncthing");
}

#[test]
fn test_sync_timings() {
    assert_eq!(RESCAN_INTERVAL, Duration::from_secs(60));
    assert_eq!(KEEPALIVE_INTERVAL, Duration::from_secs(20));
    assert_eq!(MAX_DISCOVERED_PEERS, 32);
    assert_eq!(MAX_PEER_SESSIONS, 4);
    assert_eq!(CONNECT_TIMEOUT, Duration::from_secs(4));
    assert_eq!(RETRY_AFTER_SESSION, Duration::from_secs(2));
    assert_eq!(RETRY_AWAITING_PAIRING, Duration::from_secs(5));
    assert_eq!(RETRY_AFTER_ERROR, Duration::from_secs(3));
    assert_eq!(NO_ADDRESS_POLL, Duration::from_millis(300));
    assert_eq!(AUTO_RECONNECT_DELAY, Duration::from_secs(2));
    assert_eq!(QUIT_WORKER_WAIT, Duration::from_secs(3));
}

#[test]
fn test_discovery_timings() {
    assert_eq!(DISCOVERY_INTERVAL, Duration::from_secs(20));
    assert_eq!(DISCOVERY_LISTEN, Duration::from_millis(1500));
    assert_eq!(DISCOVERY_SCAN_LISTEN, Duration::from_secs(2));
    assert_eq!(GLOBAL_DISCOVERY_INTERVAL, Duration::from_secs(60));
    assert_eq!(GLOBAL_DISCOVERY_TIMEOUT, Duration::from_secs(6));
}

#[test]
fn test_devices_dialog_delay_is_three_seconds_in_vblanks() {
    assert_eq!(VBLANK_HZ, 60);
    assert_eq!(DEVICES_DIALOG_SCAN_DELAY, 3 * VBLANK_HZ);
    assert_eq!(DEVICES_DIALOG_SCAN_DELAY, 180);
}

#[test]
fn test_power_timings() {
    assert_eq!(TRANSFER_GRACE, Duration::from_secs(120));
    assert_eq!(CPU_TRANSFER_GRACE, Duration::from_secs(10));
    assert_eq!(CPU_INTERACTIVE_GRACE, Duration::from_secs(3));
    assert_eq!(UPLOAD_DISPLAY_GRACE, Duration::from_secs(5));
    assert_eq!(POWER_TICK_INTERVAL, Duration::from_secs(10));
}

#[test]
fn test_frame_pacing_intervals_in_vblanks() {
    assert_eq!(ANIMATION_INTERVAL, 2, "30 FPS");
    assert_eq!(BACKGROUND_INTERVAL, 6, "10 FPS");
    assert_eq!(IDLE_REFRESH_INTERVAL, VBLANK_HZ, "1 FPS");
}

#[test]
fn test_screensaver_timeouts() {
    assert_eq!(SCREENSAVER_TIMEOUT_SYNCING, Duration::from_secs(10));
    assert_eq!(SCREENSAVER_TIMEOUT_IDLE, Duration::from_secs(30));
}

#[test]
fn test_input_repeat_and_drag_threshold() {
    assert_eq!(KEY_REPEAT_DELAY, 24, "0.4 s");
    assert_eq!(KEY_REPEAT_INTERVAL, 4, "15 rows per second");
    assert_eq!(TOUCH_DRAG_THRESHOLD, 12);
}

#[test]
fn test_protocol_log_limits() {
    assert_eq!(LOG_BUFFER_LINES, 200);
    assert_eq!(LOG_MAX_BYTES, 512 * 1024);
    assert_eq!(LOG_HISTORY_MAX_LINES, 2000);
    assert!(LOG_HISTORY_MAX_LINES > LOG_BUFFER_LINES);
}

#[test]
fn test_new_share_presets_and_browse_entry_last() {
    assert_eq!(NEW_SHARE_TEMPLATES.len(), 6);
    assert_eq!(
        NEW_SHARE_TEMPLATES[0],
        ("vita-savedata", "Vita Save Data", "ux0:user/00/savedata")
    );
    assert_eq!(
        NEW_SHARE_TEMPLATES[1],
        ("screenshots", "Vita Screenshots", "ux0:picture/SCREENSHOT")
    );
    assert_eq!(
        NEW_SHARE_TEMPLATES[2],
        (
            "retroarch-saves",
            "RetroArch Savefiles",
            "ux0:data/retroarch/savefiles"
        )
    );
    assert_eq!(
        NEW_SHARE_TEMPLATES[3],
        (
            "retroarch-savetate",
            "RetroArch Savestates files",
            "ux0:data/retroarch/savestates"
        )
    );
    assert_eq!(
        NEW_SHARE_TEMPLATES[4],
        ("music", "Music folder", "ux0:music")
    );
    assert_eq!(NEW_SHARE_TEMPLATES[5].0, BROWSE_TEMPLATE_ID);
    assert_eq!(NEW_SHARE_TEMPLATES[5].2, "ux0:");
}
