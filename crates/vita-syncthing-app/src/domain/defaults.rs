//! Application defaults: every value that tunes how the app behaves, in one place.
//!
//! Timings, limits and presets used to be scattered as literals across the sync worker, the
//! main loop and the policies. Change them here; the modules that apply them only import
//! these constants. (Protocol-internal timings of `syncthing-core` stay next to the code that
//! uses them; the ones an application may tune are passed through `SyncTuning`.)

use std::time::Duration;

// --- Synchronization -----------------------------------------------------------------------

/// Device name announced to peers (`Hello`, `ClusterConfig`); peers show it until renamed.
pub const DEVICE_NAME: &str = "PS-Vita";
/// Client name announced to peers (`Hello`).
pub const CLIENT_NAME: &str = "vita-syncthing";

/// Interval between automatic rescans of the shared folders (the sidebar sync card rescans immediately).
/// A rescan walks every shared folder for each connected device.
pub const RESCAN_INTERVAL: Duration = Duration::from_secs(60);
/// Maximum silence on a connection before a keepalive `Ping` is sent.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
/// Maximum number of unpaired LAN peers listed from discovery announcements (which are not
/// authenticated: anyone on the LAN can announce any number of IDs).
pub const MAX_DISCOVERED_PEERS: usize = 32;
/// Maximum number of devices synchronized at the same time (one session thread each).
pub const MAX_PEER_SESSIONS: usize = 4;
/// TCP connect / TLS handshake timeout when dialing a device.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// Delay before reconnecting to a device after an established session ended.
pub const RETRY_AFTER_SESSION: Duration = Duration::from_secs(2);
/// Delay before retrying a device that has not accepted the PS Vita yet.
pub const RETRY_AWAITING_PAIRING: Duration = Duration::from_secs(5);
/// Delay before retrying a device after a failed connection attempt.
pub const RETRY_AFTER_ERROR: Duration = Duration::from_secs(3);
/// Poll interval while a device has no known address yet (waiting for LAN discovery).
pub const NO_ADDRESS_POLL: Duration = Duration::from_millis(300);
/// Delay before the main loop restarts a sync worker that stopped on its own.
pub const AUTO_RECONNECT_DELAY: Duration = Duration::from_secs(2);
/// How long quitting waits for the sync worker to stop cleanly.
pub const QUIT_WORKER_WAIT: Duration = Duration::from_secs(3);

// --- LAN discovery -------------------------------------------------------------------------

/// Interval between two Local Discovery announcements while sessions are running.
pub const DISCOVERY_INTERVAL: Duration = Duration::from_secs(20);
/// How long each periodic discovery cycle listens for announcements.
pub const DISCOVERY_LISTEN: Duration = Duration::from_millis(1500);
/// How long the one-shot LAN scan listens when no device is linked yet.
pub const DISCOVERY_SCAN_LISTEN: Duration = Duration::from_secs(2);
/// VBlanks the `ManageDevices` dialog of a share that already has a linked device stays open
/// before the LAN is scanned (3 s; cancelled when the dialog closes). A share without a linked
/// device scans as soon as the dialog opens.
pub const DEVICES_DIALOG_SCAN_DELAY: u32 = 3 * VBLANK_HZ;
/// Interval between Global Discovery v2 lookups for WAN / relay endpoints.
pub const GLOBAL_DISCOVERY_INTERVAL: Duration = Duration::from_secs(60);
/// Timeout for HTTPS Global Discovery v2 lookups and Relay Protocol v1 handshakes.
pub const GLOBAL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(6);

// --- Power ---------------------------------------------------------------------------------

/// How long standby stays blocked after the last transfer activity.
pub const TRANSFER_GRACE: Duration = Duration::from_secs(120);
/// How long the CPU stays at full speed after the last transfer activity. Covers the short
/// pauses between files (index exchange, block requests) so the clock does not oscillate.
pub const CPU_TRANSFER_GRACE: Duration = Duration::from_secs(10);
/// How long the CPU stays at medium speed after the last user input (smooth navigation).
pub const CPU_INTERACTIVE_GRACE: Duration = Duration::from_secs(3);
/// How long the sidebar card and the screensaver keep showing "Uploading" after the last block
/// served to a peer (uploads have no end event: a peer simply stops requesting blocks).
pub const UPLOAD_DISPLAY_GRACE: Duration = Duration::from_secs(5);
/// Interval at which the standby block and the CPU clock are re-applied.
pub const POWER_TICK_INTERVAL: Duration = Duration::from_secs(10);

// --- Display -------------------------------------------------------------------------------

/// Display refresh rate of the PS Vita screen (VBlanks per second).
pub const VBLANK_HZ: u32 = 60;
/// VBlanks between frames of a continuous animation (30 FPS).
pub const ANIMATION_INTERVAL: u32 = 2;
/// Minimum VBlanks between frames triggered by background state changes (10 FPS).
pub const BACKGROUND_INTERVAL: u32 = 6;
/// VBlanks between safety refreshes of a static screen (1 FPS).
pub const IDLE_REFRESH_INTERVAL: u32 = VBLANK_HZ;
/// Inactivity before the OLED screensaver starts while a sync is in progress.
pub const SCREENSAVER_TIMEOUT_SYNCING: Duration = Duration::from_secs(10);
/// Inactivity before the OLED screensaver starts when idle.
pub const SCREENSAVER_TIMEOUT_IDLE: Duration = Duration::from_secs(30);

// --- Input -------------------------------------------------------------------------------

/// VBlanks `Up` / `Down` must be held before it starts repeating (0.4 s).
pub const KEY_REPEAT_DELAY: u32 = 24;
/// VBlanks between two repeats of a held `Up` / `Down` (15 rows per second).
pub const KEY_REPEAT_INTERVAL: u32 = 4;
/// Distance in pixels a finger must move before a touch becomes a drag (and is no tap).
pub const TOUCH_DRAG_THRESHOLD: i32 = 12;

// --- Protocol log ----------------------------------------------------------------------------

/// Lines of this session's protocol log kept in memory (`AppState::logs`, log viewer).
pub const LOG_BUFFER_LINES: usize = 200;
/// Size above which `syncthing.log` is rotated to `syncthing.log.1` (replacing the previous
/// one), so the log never uses more than about twice this size on the memory card.
pub const LOG_MAX_BYTES: u64 = 512 * 1024;
/// Newest `syncthing.log` lines loaded by the log viewer's `Show All` (bounds its memory use).
pub const LOG_HISTORY_MAX_LINES: usize = 2000;

// --- Shares --------------------------------------------------------------------------------

/// Folder ID of the `NEW_SHARE_TEMPLATES` entry that opens the folder browser.
pub const BROWSE_TEMPLATE_ID: &str = "__browse__";

/// Share presets offered in the `Create New Share` dialog (and listed first at the root of the
/// `Files` browser while not shared): `(folder ID, label, target path)`.
/// The `__browse__` entry opens the folder browser instead of creating a share.
pub const NEW_SHARE_TEMPLATES: &[(&str, &str, &str)] = &[
    ("screenshots", "Vita Screenshots", "ux0:picture/SCREENSHOT"),
    (
        "retroarch-saves",
        "RetroArch Savefiles",
        "ux0:data/retroarch/savefiles",
    ),
    (
        "retroarch-savetate",
        "RetroArch Savestates files",
        "ux0:data/retroarch/savestates",
    ),
    ("music", "Music folder", "ux0:music"),
    (
        "__browse__",
        "Browse ux0: to pick any custom folder...",
        "ux0:",
    ),
];
