//! English (`en`) string catalog for VitaSyncthing.

use super::{Messages, Strings};
use crate::domain::Language;

pub static STRINGS_EN: Strings = Strings {
    messages: &MESSAGES_EN,
    language: Language::English,

    // Common Actions
    action_cancel: "Cancel",
    action_save: "Save",
    action_delete: "Delete",
    action_done: "Done",
    action_close: "Close",
    action_confirm: "Confirm",
    action_quit: "Quit",
    action_quit_app: "Quit App",
    action_back: "Back",
    action_accept: "Accept",
    action_decline: "Decline",
    action_later: "Later",
    action_up: "Up",
    action_open: "Open",
    action_share: "Share",
    action_shared: "Shared",
    action_linked: "Linked",
    action_not_linked: "Not linked",
    action_pause: "Pause",
    action_enable: "Enable",
    action_devices: "Devices",
    action_folder: "Folder",
    action_folder_path: "Folder Path",
    action_add_by_id: "Add by ID",
    action_add_device: "Add Device",
    action_add_device_id: "Add Device ID",
    action_add_remote_device: "Add Remote Device",
    action_apply_sync: "Apply & Sync",
    action_scan_qr: "Scan QR",
    action_new_share: "New Share",
    action_qr_id: "QR / ID",
    action_switch_theme: "Switch Theme",
    action_clear_declined: "Clear Declined",
    action_virtual_keyboard: "Virtual Keyboard",
    action_hide_keyboard: "Hide Keyboard",
    action_backspace: "Backspace",

    // Status Badges & Chips
    status_ready: "Ready",
    status_syncing: "Syncing",
    status_scanning: "Scanning",
    status_active: "Active",
    status_paused: "Paused",

    // Navigation & Sidebar
    tab_overview: "Overview",
    tab_files: "Files",
    tab_settings: "Settings",
    app_title: "VitaSync",
    app_version_label: "v0.1.0",
    sidebar_syncing: "Syncing...",
    sidebar_last_sync: "Last sync session",
    protocol_tls: "BEP/1.0 TLS 1.3",
    protocol_discovery: "UDP 21027...",

    // Controller Hints
    hint_navigate: "Navigate",
    hint_select: "Select",
    hint_toggle: "Toggle",
    hint_back: "Back",
    hint_tabs: "Tabs",
    hint_quit: "Quit",
    hint_move: "Move",
    hint_screensaver: "Screensaver",
    hint_folder: "Folder",
    hint_folder_path: "Folder Path",
    hint_share_folder: "Share Folder",
    hint_select_target: "Select Target",
    hint_open_subfolder: "Open Subfolder",
    hint_up_one_level: "Up One Level",
    hint_up_back: "Up / Back",
    hint_back_to_device_id: "Back to Device ID",
    hint_pause_enable: "Pause/Enable",
    hint_add_by_id: "Add by ID",
    hint_scan_qr: "Scan QR",
    hint_apply_sync: "Apply & Sync",
    hint_confirm_delete: "Confirm Delete",
    hint_confirm_quit: "Confirm Quit",
    hint_create_pick_devices: "Create & Pick Devices",
    hint_add_remote_device_id: "Add Remote Device ID",
    hint_close_qr_device_id: "Close QR / Device ID",
    hint_accept_share: "Accept share",
    hint_confirm_add: "Confirm & Add",
    hint_type_key: "Type Key",

    // Screen 1: Dashboard (Overview)
    dashboard_title: "Overview",
    hero_this_device: "This device",
    hero_device_name: "PlayStation Vita",
    hero_device_id_label: "Device ID",

    // Screen 2: Shared Folders
    msg_no_shares: "No shared folders configured yet",
    msg_no_shares_hint_1:
        "Press Triangle (New Share) to create a share on your PS Vita and link a device,",
    msg_no_shares_hint_2: "or tap the sync card to import folders shared by a remote peer.",

    // Screen 3: Files & Folder Picker
    picker_title_create: "Share a Folder",
    picker_subtitle_create: "Select a folder to share it, open it to browse its subfolders",
    picker_title_choose: "Choose Target Folder",
    picker_default_share: "Default share",

    // Screen 4: Settings & Protocol Log
    settings_title: "Settings & Log",
    settings_subtitle: "Display & BEP/1.0",
    settings_sec_display: "DISPLAY AND OLED SAVER",
    settings_label_colour_theme: "Colour Theme",
    settings_desc_theme_dark: "Dark — Blue-grey look with teal accent",
    settings_desc_theme_oled: "OLED — Pure black background (saves PS Vita OLED battery)",
    settings_saver_auto_hint:
        "OLED Screensaver activates automatically after 12s during sync (or press SELECT anytime).",
    settings_sec_devices: "DEVICES & PAIRING",
    settings_desc_devices: "Pair a remote device or view this PS Vita's ID",
    settings_sec_log: "BEP/1.0 & TLS 1.3 PROTOCOL LOG",
    settings_sec_declined: "DECLINED FOLDERS",
    settings_no_declined: "None · folders offered by peers are always asked first",
    settings_sec_language: "LANGUAGE",
    settings_language_system: "System default",
    action_change_language: "Change Language",

    // Screensaver
    saver_wake_hint: "Touch screen or press any button to wake",

    // Modals: Create Share
    modal_create_share_title: "Create New Share",
    modal_create_share_subtitle:
        "Choose a PS Vita folder preset or browse ux0: to create a new Syncthing share",
    modal_create_share_browse_sub: "Open the interactive ux0: directory browser",

    // Modals: Manage Devices
    modal_manage_devices_default_title: "Share",
    modal_manage_devices_subtitle_incomplete:
        "Link at least one device to finish creating this share",
    modal_manage_devices_subtitle:
        "Tick the remote Syncthing devices that should synchronize this folder",
    modal_manage_devices_scanning: "Scanning LAN (UDP 21027 + TCP 22000) for Syncthing peers...",
    modal_manage_devices_empty: "No remote Syncthing devices discovered yet.",
    modal_manage_devices_empty_hint:
        "Press Square (Add by ID) to enter a Short/Full ID, or Triangle to scan LAN.",

    // Modals: Add Device
    modal_add_device_title: "Add Remote Device by ID",
    modal_add_device_subtitle:
        "Enter the 7-character Short ID (recommended) or full 56-character Syncthing Device ID",
    modal_add_device_example: "e.g. MFZWI3D  or  MFZWI3D-BONSGYC-... (tap to type)",
    modal_add_device_tip:
        "Tip: You only need the first 7 characters (Short ID shown in Syncthing Web UI).",
    modal_add_device_placeholder: "Enter 7-char Short ID or 56-char Full ID...",
    modal_add_device_name_label: "Device Name (optional):",
    modal_add_device_name_empty: "(tap to enter name)",
    modal_add_device_confirm: "Add Device",

    // Modals: Confirm Delete
    modal_delete_share_title: "Delete this share?",
    modal_delete_share_subtitle: "The folder stops synchronizing with all linked devices",
    modal_delete_share_decline_hint:
        "Peers' offers of it are declined (undo in Settings · Clear Declined).",

    // Modals: Confirm Quit
    modal_quit_title: "Quit VitaSync?",
    modal_quit_subtitle: "Confirm whether to close the application and return to LiveArea",
    modal_quit_syncing_warn: "A BEP/1.0 synchronization is currently in progress.",
    modal_quit_syncing_hint: "Quitting now will interrupt active file transfers.",
    modal_quit_idle_ready: "Are you sure you want to exit VitaSync?",
    modal_quit_idle_hint: "All share configurations and device settings are saved on ux0:.",

    // Modals: Device ID
    modal_device_id_title: "PS Vita Syncthing Device ID",
    modal_device_id_subtitle:
        "Scan this QR code in Syncthing (Add Remote Device) or enter the 56-character ID",
    modal_device_id_scan_web: "Scan QR in Syncthing Web / Mobile UI",
    modal_device_id_dev_name: "Device Name: PlayStation Vita (vita-syncthing)",
    modal_device_id_cert_path: "Identity Cert: ux0:data/syncthing/cert.der",
    modal_device_id_short_label: "Short ID (LAN discovery):",
    modal_device_id_full_label: "Full 56-Character Device ID:",
    modal_device_id_scan_hint:
        "On phone/PC: Syncthing > Devices > Add Device > Tap QR icon to scan",

    // Modals: Folder Offer
    modal_folder_offer_title: "New shared folder",
    modal_folder_offer_hint:
        "Accept to pick a folder on ux0:. Nothing syncs until you enable the share.",

    // Modals: Scan QR
    modal_scan_qr_title: "Scan Device ID QR Code",
    modal_scan_qr_subtitle: "Syncthing GUI: Actions > Show ID, then aim the rear camera at it",
};

/// EN templates for `UiText` messages.
pub static MESSAGES_EN: Messages = Messages {
    ready_tap_sync_card: "Ready · Tap the sync card to synchronize",
    ready_manual_pairing: "Ready (manual LAN pairing)",
    no_peer_linked: "No peer linked",
    select_device_to_connect: "Select or add a device in a share's Devices to connect",
    no_folders_synced: "No folders synced yet",
    no_remote_folders: "No remote folders shared",
    starting_sync: "Starting synchronization...",
    scanning_lan: "Scanning LAN (UDP 21027) for Syncthing peers...",
    connecting_to: "Connecting over TLS 1.3 to {name} ({addr})...",
    exchanging_index: "Exchanging ClusterConfig & BEP Index...",
    file_up_to_date: "Up to date: {file}",
    downloading: "Downloading {file} ({pct}%)",
    index_published: "Local Vita index published ({files} files)",
    uploading: "Uploading to peer: {file}",
    sync_finished: "Finished",
    connections_one: "Connected (1 connection)",
    connections_many: "Connected (1+{secondary} connections)",
    sync_summary: "{connections} · {up_to_date} up to date | {downloaded} DL ({kb} KB) | {uploaded} UL ({bytes} B)",
    found_lan_peers: "Found {count} LAN peer(s). Select a device in a share's Devices to connect.",
    no_peer_add_device: "No peer linked. Add a device in a share's Devices or Settings.",
    discovery_error: "LAN discovery error: {error}",
    syncing_with_peers: "Synchronizing with LAN peers...",
    theme_dark: "Theme: Dark (blue-grey)",
    theme_oled: "Theme: OLED (pure black #000000)",
    language_changed: "Language: {name}",
    device_linked: "Added {device} to '{share}'",
    device_unlinked: "Removed {device} from '{share}'",
    link_device_first: "Link at least one device to create this share",
    added_short_id: "Added Short ID {prefix} (will auto-pair on LAN)",
    added_device: "Added device {name} ({short})",
    own_device_id: "Cannot add this PS Vita's own Device ID",
    qr_not_device_id: "This QR code is not a Syncthing Device ID",
    share_deleted: "Deleted share '{label}'",
    link_device_to_share: "Link a device to share '{label}'",
    share_created: "Created share '{label}'",
    share_not_created: "Share '{label}' not created: no device linked",
    saved_in: "Saved in {path}",
    share_active: "Share {id} is now active",
    share_paused: "Share {id} is now paused",
    offer_accepted: "Accepted share '{label}' ({id})",
    choose_where_to_store: "Choose where to store '{label}'",
    offer_declined: "Declined share '{label}' ({id})",
    no_declined_folders: "No declined folders",
    declined_cleared: "Cleared {count} declined folder(s): they will be offered again",
    id_empty: "Device ID cannot be empty",
    id_invalid_char: "Invalid character '{ch}' (use Base32 A-Z, 2-7)",
    id_too_short: "Enter at least 7 characters (Short ID) or the full 56-character ID",
    id_too_long: "Device ID is too long ({len} characters, max 56)",
    id_checksum: "Typo in groups {first}-{second} (check digit mismatch)",
    id_decode_failed: "Device ID could not be decoded",
    no_devices_linked: "No devices linked (press Cross to add a device)",
    shared_with: "Shared with: {devices}",
    picker_share_this_folder: "Share this folder ({path})",
    picker_keep_current: "Keep current folder ({path})",
    picker_save_data: "Save Data ({path})",
    picker_pictures: "PS Vita Pictures ({path})",
    picker_music: "PS Vita Music ({path})",
    picker_retroarch: "RetroArch save files ({path})",
    picker_default_folder: "Default Syncthing folder ({path})",
    picker_browsed_folder: "Use current browsed folder ({path})",
    device_id_section: "SYNCTHING DEVICE ID (8 GROUPS OF 7 CHARS)",
    device_id_groups_1_4: "GROUPS 1 - 4",
    device_id_groups_5_8: "GROUPS 5 - 8",
    short_id_label: "Short ID: {id}",
    camera_starting: "Starting rear camera...",
    camera_error: "{error} · Enter the ID manually instead",
    add_device_matches_peer: "Matches discovered LAN peer: {name} ({short} @ {addr}) · Ready to link!",
    add_device_valid_short: "Valid Short ID ({len} chars: {short}) · Will auto-pair with the matching LAN peer",
    add_device_valid_full: "Valid full 56-character Syncthing Device ID ({short})",
    device_row_short: "Short ID: {prefix} (resolved on LAN)  ·  Address: {addr}",
    device_row_full: "ID: {id}  ·  Address: {addr}",
    unknown_device: "Device {short}",
};
