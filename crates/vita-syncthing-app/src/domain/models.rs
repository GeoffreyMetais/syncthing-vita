//! Pure domain models and value objects for PS Vita Syncthing.

use syncthing_core::DeviceId;

/// Number of visible rows in the `ux0:` directory browser.
pub const BROWSER_ROWS: usize = 8;

/// Colour theme modes supported by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark = 0,
    Oled = 1,
}

/// Main navigation tabs in the left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Dashboard = 0,
    Shares = 1,
    FolderPicker = 2,
    Settings = 3,
}

/// Mode for the `Files` (`FolderPicker`) tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    /// Change target directory for `selected_share_idx`.
    ChangeShareTarget,
    /// Create a brand-new Syncthing share from the chosen directory.
    CreateNewShare,
}

/// VitaSend-style modal dialogs drawn over dimmed background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalDialog {
    None,
    /// Create a new share from a template or by browsing `ux0:`.
    CreateShare {
        selected_idx: usize,
    },
    /// Add or remove remote Syncthing devices for a share.
    ManageDevices {
        share_idx: usize,
        selected_dev_idx: usize,
    },
    /// Enter a remote Syncthing Device ID (7+ char Short ID or full 56-char ID) via PS Vita
    /// System Keyboard (`SceIme`) or the built-in Base32 keypad.
    AddDeviceById {
        return_share_idx: Option<usize>,
        keypad_idx: usize,
    },
    /// Scan a remote device's Syncthing Device ID QR code with the rear camera (`SceCamera`).
    /// Closing it returns to `AddDeviceById` with the same `return_share_idx`.
    ScanDeviceQr {
        return_share_idx: Option<usize>,
    },
    /// Display the PS Vita's full Syncthing Device ID and scannable QR code.
    ShowDeviceId,
    /// Confirmation dialog before quitting the application (`START` button).
    ConfirmQuit,
    /// Confirmation dialog before removing `shares[share_idx]` from the configuration.
    ConfirmDeleteShare {
        share_idx: usize,
    },
}

/// Known or discovered remote Syncthing device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDevice {
    pub device_id: DeviceId,
    pub name: String,
    pub address: String,
}

/// Syncthing shared folder configuration on PS Vita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareConfigEntry {
    pub folder_id: String,
    pub label: String,
    pub target_path: String,
    pub enabled: bool,
    /// Hex strings (`DeviceId::to_hex()`) of remote devices this folder is shared with.
    pub shared_devices: Vec<String>,
}

/// Latest greyscale camera frame shown in the `ScanDeviceQr` viewfinder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CameraPreview {
    pub width: usize,
    pub height: usize,
    /// `width * height` luma bytes, row-major. Empty until the first frame arrives.
    pub gray: Vec<u8>,
}

impl CameraPreview {
    pub fn has_frame(&self) -> bool {
        self.width > 0 && self.height > 0 && self.gray.len() == self.width * self.height
    }

    pub fn clear(&mut self) {
        self.width = 0;
        self.height = 0;
        self.gray.clear();
    }
}

/// Item in the target folder picker (`ux0:`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub label: String,
    pub path: String,
    pub is_preset: bool,
    /// True if another share already targets this folder: shown greyed out and not selectable.
    pub is_shared: bool,
}

/// Templates offered in the `Create New Share` modal dialog.
pub const NEW_SHARE_TEMPLATES: &[(&str, &str, &str)] = &[
    ("vita-savedata", "Vita Save Data", "ux0:user/00/savedata"),
    (
        "vita-pictures",
        "Vita Screenshots & Pictures",
        "ux0:picture",
    ),
    ("vita-music", "Vita Music Library", "ux0:music"),
    (
        "retroarch-saves",
        "RetroArch Savefiles",
        "ux0:data/retroarch/savefiles",
    ),
    (
        "vita-shared",
        "Vita Shared Folder",
        "ux0:data/syncthing/shared",
    ),
    (
        "__browse__",
        "Browse ux0: to pick any custom folder...",
        "ux0:",
    ),
];

/// 36-key (9x4) Base32 quick-keypad for `ModalDialog::AddDeviceById`.
pub const ADD_DEVICE_KEYPAD: &[&str; 36] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "-", "2", "3", "4", "5", "6", "7", "Bksp", "Clear", "Add",
];

/// Format byte sizes cleanly ("1.5 KB", "3.2 MB").
pub fn fmt_size(b: usize) -> String {
    if b < 1024 {
        format!("{} B", b)
    } else if b < 1024 * 1024 {
        format!("{:.1} KB", (b as f64) / 1024.0)
    } else {
        format!("{:.1} MB", (b as f64) / (1024.0 * 1024.0))
    }
}
