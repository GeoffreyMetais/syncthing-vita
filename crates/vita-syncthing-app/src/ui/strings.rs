//! Factorized UI string resources and label constants for VitaSyncthing.
//!
//! Centralizes user-facing copy (actions, tab titles, screen titles,
//! button hints, modal dialog prompts, and formatting helpers) into a
//! zero-allocation static catalog.

pub use crate::ui::i18n::*;

// ============================================================================
// Common Actions
// ============================================================================

pub const ACTION_CANCEL: &str = "Cancel";
pub const ACTION_SAVE: &str = "Save";
pub const ACTION_DELETE: &str = "Delete";
pub const ACTION_DONE: &str = "Done";
pub const ACTION_CLOSE: &str = "Close";
pub const ACTION_CONFIRM: &str = "Confirm";
pub const ACTION_QUIT: &str = "Quit";
pub const ACTION_QUIT_APP: &str = "Quit App";
pub const ACTION_BACK: &str = "Back";
pub const ACTION_ACCEPT: &str = "Accept";
pub const ACTION_DECLINE: &str = "Decline";
pub const ACTION_LATER: &str = "Later";
pub const ACTION_UP: &str = "Up";
pub const ACTION_OPEN: &str = "Open";
pub const ACTION_SHARE: &str = "Share";
pub const ACTION_SHARED: &str = "Shared";
pub const ACTION_LINKED: &str = "Linked";
pub const ACTION_NOT_LINKED: &str = "Not linked";
pub const ACTION_PAUSE: &str = "Pause";
pub const ACTION_ENABLE: &str = "Enable";
pub const ACTION_DEVICES: &str = "Devices";
pub const ACTION_FOLDER: &str = "Folder";
pub const ACTION_FOLDER_PATH: &str = "Folder Path";
pub const ACTION_ADD_BY_ID: &str = "Add by ID";
pub const ACTION_ADD_DEVICE: &str = "Add Device";
pub const ACTION_ADD_DEVICE_ID: &str = "Add Device ID";
pub const ACTION_ADD_REMOTE_DEVICE: &str = "Add Remote Device";
pub const ACTION_SCAN_QR: &str = "Scan QR";
pub const ACTION_NEW_SHARE: &str = "New Share";
pub const ACTION_QR_ID: &str = "QR / ID";
pub const ACTION_SWITCH_THEME: &str = "Switch Theme";
pub const ACTION_CLEAR_DECLINED: &str = "Clear Declined";
pub const ACTION_VIRTUAL_KEYBOARD: &str = "Virtual Keyboard";
pub const ACTION_HIDE_KEYBOARD: &str = "Hide Keyboard";
pub const ACTION_BACKSPACE: &str = "Backspace";

// ============================================================================
// Status Badges & Chips
// ============================================================================

pub const STATUS_READY: &str = "Ready";
pub const STATUS_SYNCING: &str = "Syncing";
pub const STATUS_SCANNING: &str = "Scanning";
pub const STATUS_ACTIVE: &str = "Active";
pub const STATUS_PAUSED: &str = "Paused";

// ============================================================================
// Navigation & Sidebar
// ============================================================================

pub const TAB_OVERVIEW: &str = "Overview";
pub const TAB_SHARES: &str = "Shares";
pub const TAB_FILES: &str = "Files";
pub const TAB_SETTINGS: &str = "Settings";
pub const APP_TITLE: &str = "VitaSync";
pub const APP_VERSION_LABEL: &str = "v0.1.0";
pub const SIDEBAR_SYNCING: &str = "Syncing...";
pub const SIDEBAR_LAST_SYNC: &str = "Last sync session";
pub const PROTOCOL_TLS: &str = "BEP/1.0 TLS 1.3";
pub const PROTOCOL_DISCOVERY: &str = "UDP 21027...";

// ============================================================================
// Controller Hints (Footer Bar)
// ============================================================================

pub const HINT_NAVIGATE: &str = "Navigate";
pub const HINT_SELECT: &str = "Select";
pub const HINT_TOGGLE: &str = "Toggle";
pub const HINT_BACK: &str = "Back";
pub const HINT_TABS: &str = "Tabs";
pub const HINT_QUIT: &str = "Quit";
pub const HINT_MOVE: &str = "Move";
pub const HINT_SCREENSAVER: &str = "Screensaver";
pub const HINT_FOLDER: &str = "Folder";
pub const HINT_FOLDER_PATH: &str = "Folder Path";
pub const HINT_SHARE_FOLDER: &str = "Share Folder";
pub const HINT_SELECT_TARGET: &str = "Select Target";
pub const HINT_OPEN_SUBFOLDER: &str = "Open Subfolder";
pub const HINT_UP_ONE_LEVEL: &str = "Up One Level";
pub const HINT_UP_BACK: &str = "Up / Back";
pub const HINT_BACK_TO_DEVICE_ID: &str = "Back to Device ID";
pub const HINT_PAUSE_ENABLE: &str = "Pause/Enable";
pub const HINT_ADD_BY_ID: &str = "Add by ID";
pub const HINT_SCAN_QR: &str = "Scan QR";
pub const HINT_CONFIRM_DELETE: &str = "Confirm Delete";
pub const HINT_CONFIRM_QUIT: &str = "Confirm Quit";
pub const HINT_CREATE_PICK_DEVICES: &str = "Create & Pick Devices";
pub const HINT_ADD_REMOTE_DEVICE_ID: &str = "Add Remote Device ID";
pub const HINT_CLOSE_QR_DEVICE_ID: &str = "Close QR / Device ID";
pub const HINT_ACCEPT_SHARE: &str = "Accept share";
pub const HINT_CONFIRM_ADD: &str = "Confirm & Add";
pub const HINT_TYPE_KEY: &str = "Type Key";

// ============================================================================
// Screen 1: Dashboard (Overview)
// ============================================================================

pub const DASHBOARD_TITLE: &str = "Overview";
pub const HERO_THIS_DEVICE: &str = "This device";
pub const HERO_DEVICE_NAME: &str = "PlayStation Vita";
pub const HERO_DEVICE_ID_LABEL: &str = "Device ID";
pub const SECTION_RECENT_ACTIVITY: &str = "Recent activity";
pub const MSG_NO_SYNC_EVENTS: &str = "No synchronization events recorded yet.";

// ============================================================================
// Screen 2: Shared Folders
// ============================================================================

pub const SHARES_TITLE: &str = "Shared Folders";
pub const MSG_NO_SHARES: &str = "No shared folders configured yet";
pub const MSG_NO_SHARES_HINT_1: &str =
    "Press Triangle (New Share) to create a share on your PS Vita and link a device,";
pub const MSG_NO_SHARES_HINT_2: &str =
    "or tap the sync card to import folders shared by a remote peer.";

// ============================================================================
// Screen 3: Files & Folder Picker
// ============================================================================

pub const PICKER_TITLE_CREATE: &str = "Share a Folder";
pub const PICKER_SUBTITLE_CREATE: &str =
    "Select a folder to share it, open it to browse its subfolders";
pub const PICKER_TITLE_CHOOSE: &str = "Choose Target Folder";
pub const PICKER_DEFAULT_SHARE: &str = "Default share";

// ============================================================================
// Screen 4: Settings & Protocol Log
// ============================================================================

pub const SETTINGS_TITLE: &str = "Settings & Log";
pub const SETTINGS_SUBTITLE: &str = "Display & BEP/1.0";
pub const SETTINGS_SEC_DISPLAY: &str = "DISPLAY AND OLED SAVER";
pub const SETTINGS_LABEL_COLOUR_THEME: &str = "Colour Theme";
pub const SETTINGS_DESC_THEME_DARK: &str = "Dark — VitaSend slate blue-grey look with teal accent";
pub const SETTINGS_DESC_THEME_OLED: &str =
    "OLED — Pure black background (saves PS Vita OLED battery)";
pub const SETTINGS_SAVER_AUTO_HINT: &str =
    "OLED Screensaver activates automatically after 12s during sync (or press SELECT anytime).";
pub const SETTINGS_SEC_LOG: &str = "BEP/1.0 & TLS 1.3 PROTOCOL LOG";
pub const SETTINGS_SEC_DECLINED: &str = "DECLINED FOLDERS";
pub const SETTINGS_NO_DECLINED: &str = "None · folders offered by peers are always asked first";

// ============================================================================
// Screensaver
// ============================================================================

pub const SAVER_WAKE_HINT: &str = "Touch screen or press any button to wake";

// ============================================================================
// Modals
// ============================================================================

// Create Share Modal
pub const MODAL_CREATE_SHARE_TITLE: &str = "Create New Share";
pub const MODAL_CREATE_SHARE_SUBTITLE: &str =
    "Choose a PS Vita folder preset or browse ux0: to create a new Syncthing share";
pub const MODAL_CREATE_SHARE_BROWSE_SUB: &str = "Open the interactive ux0: directory browser";

// Manage Devices Modal
pub const MODAL_MANAGE_DEVICES_DEFAULT_TITLE: &str = "Share";
pub const MODAL_MANAGE_DEVICES_SUBTITLE_INCOMPLETE: &str =
    "Link at least one device to finish creating this share";
pub const MODAL_MANAGE_DEVICES_SUBTITLE: &str =
    "Tick the remote Syncthing devices that should synchronize this folder";
pub const MODAL_MANAGE_DEVICES_SCANNING: &str =
    "Scanning LAN (UDP 21027 + TCP 22000) for Syncthing peers...";
pub const MODAL_MANAGE_DEVICES_EMPTY: &str = "No remote Syncthing devices discovered yet.";
pub const MODAL_MANAGE_DEVICES_EMPTY_HINT: &str =
    "Press Square (Add by ID) to enter a Short/Full ID, or Triangle to scan LAN.";

// Add Device Modal
pub const MODAL_ADD_DEVICE_TITLE: &str = "Add Remote Device by ID";
pub const MODAL_ADD_DEVICE_SUBTITLE: &str =
    "Enter the 7-character Short ID (recommended) or full 56-character Syncthing Device ID";
pub const MODAL_ADD_DEVICE_EXAMPLE: &str = "e.g. MFZWI3D  or  MFZWI3D-BONSGYC-... (tap to type)";
pub const MODAL_ADD_DEVICE_TIP: &str =
    "Tip: You only need the first 7 characters (Short ID shown in Syncthing Web UI).";
pub const MODAL_ADD_DEVICE_PLACEHOLDER: &str = "Enter 7-char Short ID or 56-char Full ID...";
pub const MODAL_ADD_DEVICE_NAME_LABEL: &str = "Device Name (optional):";
pub const MODAL_ADD_DEVICE_NAME_EMPTY: &str = "(tap to enter name)";
pub const MODAL_ADD_DEVICE_CONFIRM: &str = "Add Device";

// Confirm Delete Share Modal
pub const MODAL_DELETE_SHARE_TITLE: &str = "Delete this share?";
pub const MODAL_DELETE_SHARE_SUBTITLE: &str =
    "The folder stops synchronizing with all linked devices";
pub const MODAL_DELETE_SHARE_DECLINE_HINT: &str =
    "Peers' offers of it are declined (undo in Settings · Clear Declined).";

// Confirm Quit Modal
pub const MODAL_QUIT_TITLE: &str = "Quit VitaSync?";
pub const MODAL_QUIT_SUBTITLE: &str =
    "Confirm whether to close the application and return to LiveArea";
pub const MODAL_QUIT_SYNCING_WARN: &str = "A BEP/1.0 synchronization is currently in progress.";
pub const MODAL_QUIT_SYNCING_HINT: &str = "Quitting now will interrupt active file transfers.";
pub const MODAL_QUIT_IDLE_READY: &str = "Are you sure you want to exit VitaSync?";
pub const MODAL_QUIT_IDLE_HINT: &str =
    "All share configurations and device settings are saved on ux0:.";

// Device ID Modal
pub const MODAL_DEVICE_ID_TITLE: &str = "PS Vita Syncthing Device ID";
pub const MODAL_DEVICE_ID_SUBTITLE: &str =
    "Scan this QR code in Syncthing (Add Remote Device) or enter the 56-character ID";
pub const MODAL_DEVICE_ID_SCAN_WEB: &str = "Scan QR in Syncthing Web / Mobile UI";
pub const MODAL_DEVICE_ID_DEV_NAME: &str = "Device Name: PlayStation Vita (vita-syncthing)";
pub const MODAL_DEVICE_ID_CERT_PATH: &str = "Identity Cert: ux0:data/syncthing/cert.der";
pub const MODAL_DEVICE_ID_SHORT_LABEL: &str = "Short ID (LAN discovery):";
pub const MODAL_DEVICE_ID_FULL_LABEL: &str = "Full 56-Character Device ID:";
pub const MODAL_DEVICE_ID_SCAN_HINT: &str =
    "On phone/PC: Syncthing > Devices > Add Device > Tap QR icon to scan";

// Folder Offer Modal
pub const MODAL_FOLDER_OFFER_TITLE: &str = "New shared folder";
pub const MODAL_FOLDER_OFFER_HINT: &str =
    "Accept to pick a folder on ux0:. Nothing syncs until you enable the share.";

// Scan QR Modal
pub const MODAL_SCAN_QR_TITLE: &str = "Scan Device ID QR Code";
pub const MODAL_SCAN_QR_SUBTITLE: &str =
    "Syncthing GUI: Actions > Show ID, then aim the rear camera at it";

// ============================================================================
// Formatting Helpers
// ============================================================================

/// Formats the overview page subtitle: `"{active} active share(s) · {devices} known device(s)"`.
pub fn fmt_active_shares_devices(active: usize, devices: usize) -> String {
    format!("{active} active share(s) · {devices} known device(s)")
}

/// Formats a share card title: `"{label}  ·  {folder_id}"`.
pub fn fmt_share_title(label: &str, folder_id: &str) -> String {
    format!("{label}  ·  {folder_id}")
}

/// Formats the template subtitle: `"ID: {fid}  ·  Path: {path}"`.
pub fn fmt_template_sub(fid: &str, path: &str) -> String {
    format!("ID: {fid}  ·  Path: {path}")
}

/// Formats the short ID label: `"Short ID: {short_id}"`.
pub fn fmt_short_id_label(short_id: &str) -> String {
    format!("Short ID: {short_id}")
}

/// Formats the target folder hint when choosing folder in picker.
pub fn fmt_picker_target(label: &str, folder_id: &str) -> String {
    format!("For {label} ({folder_id})")
}

/// Formats the item counter in the breadcrumb bar: `"{current} / {total}"`.
pub fn fmt_item_counter(current: usize, total: usize) -> String {
    format!("{current} / {total}")
}

/// Formats the declined folders count and list.
pub fn fmt_declined_summary(count: usize, list: &str) -> String {
    format!("{count} declined: {list}")
}

/// Formats the files-kept message when confirming share deletion.
pub fn fmt_files_kept_on_vita(path: &str) -> String {
    format!("Files in {path} are kept on the PS Vita.")
}

/// Formats the modal title for device management of a share.
pub fn fmt_devices_for_share(title: &str) -> String {
    format!("Devices for {title}")
}

/// Formats the folder offer requester message.
pub fn fmt_folder_offer_sub(device: &str) -> String {
    format!("{device} wants to share a folder with this PS Vita")
}

/// Formats the pending folder offers count message.
pub fn fmt_offers_waiting(count: usize) -> String {
    format!("{count} more offer(s) waiting")
}

/// Formats the screensaver active syncing status line.
pub fn fmt_saver_syncing(pct: Option<u32>, current_file: &str) -> String {
    match pct {
        Some(p) => format!("Syncing {p}% · {current_file}"),
        None => format!("Syncing · {current_file}"),
    }
}

/// Formats the screensaver idle status line.
pub fn fmt_saver_idle(pct: Option<u32>, status_line: &str) -> String {
    match pct {
        Some(p) => format!("Idle ({p}%) · {status_line}"),
        None => format!("Idle · {status_line}"),
    }
}
