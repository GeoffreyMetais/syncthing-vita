//! Pure domain models and value objects for PS Vita Syncthing.

use syncthing_core::DeviceId;

use crate::domain::ui_text::UiText;

/// Devices visible in the `ManageDevices` dialog (the list does not scroll yet).
pub const VISIBLE_DEVICES: usize = 5;

/// Number of visible rows in the `ux0:` directory browser.
pub const BROWSER_ROWS: usize = 8;

/// User interface languages supported by VitaSyncthing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English = 0,
    French = 1,
    Spanish = 2,
    German = 3,
    Italian = 4,
}

impl Language {
    pub const ALL: [Language; 5] = [
        Language::English,
        Language::French,
        Language::Spanish,
        Language::German,
        Language::Italian,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "Français",
            Language::Spanish => "Español",
            Language::German => "Deutsch",
            Language::Italian => "Italiano",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Language::English => Language::French,
            Language::French => Language::Spanish,
            Language::Spanish => Language::German,
            Language::German => Language::Italian,
            Language::Italian => Language::English,
        }
    }

    /// ISO 639-1 code used in configuration files.
    pub const fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::French => "fr",
            Language::Spanish => "es",
            Language::German => "de",
            Language::Italian => "it",
        }
    }

    /// Parses an ISO 639-1 code (case-insensitive, trimmed).
    pub fn from_code(s: &str) -> Option<Self> {
        let code = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|l| l.code() == code)
    }
}

/// UI language chosen in Settings: follow the PS Vita system language, or a fixed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguagePreference {
    #[default]
    System,
    Fixed(Language),
}

impl LanguagePreference {
    /// Identifier for `language.cfg`: `system` or an ISO 639-1 code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Fixed(lang) => lang.code(),
        }
    }

    /// Parses `system` or an ISO 639-1 code (case-insensitive, trimmed).
    pub fn parse(s: &str) -> Option<Self> {
        if s.trim().eq_ignore_ascii_case("system") {
            return Some(Self::System);
        }
        Language::from_code(s).map(Self::Fixed)
    }

    /// Cycles System -> English -> Français -> Español -> Deutsch -> Italiano -> System.
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Fixed(Language::English),
            Self::Fixed(Language::Italian) => Self::System,
            Self::Fixed(lang) => Self::Fixed(lang.next()),
        }
    }

    /// Language actually displayed, given the detected system language.
    pub const fn resolve(self, system: Language) -> Language {
        match self {
            Self::System => system,
            Self::Fixed(lang) => lang,
        }
    }
}

/// Colour theme modes supported by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    Dark = 0,
    #[default]
    Oled = 1,
}

impl ThemeMode {
    /// Returns the lowercase ASCII identifier for configuration files.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Oled => "oled",
        }
    }

    /// Parses a theme mode identifier (case-insensitive, trimmed).
    /// Accepts `"dark"`, `"0"`, `"oled"`, `"1"`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dark" | "0" => Some(Self::Dark),
            "oled" | "1" => Some(Self::Oled),
            _ => None,
        }
    }
}

/// Main navigation tabs in the left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    /// `Overview`: device status card and the list of shares.
    Dashboard = 0,
    FolderPicker = 1,
    Settings = 2,
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
    /// Confirmation dialog before reverting the local changes of the read-only
    /// `shares[share_idx]` (opened from `ManageDevices`, which it returns to).
    ConfirmRevertShare {
        share_idx: usize,
    },
    /// A remote device offers to share a folder: the user accepts, declines or postpones it.
    FolderOffer {
        offer_idx: usize,
    },
}

/// Known or discovered remote Syncthing device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDevice {
    pub device_id: DeviceId,
    pub name: String,
    pub address: String,
}

/// Access mode of a share on the PS Vita.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShareMode {
    /// Local changes are sent to peers and their changes are applied (Syncthing "Send & Receive").
    #[default]
    ReadWrite,
    /// Peers' changes are applied, but local changes are never sent to them
    /// (Syncthing "Receive Only").
    ReadOnly,
}

impl ShareMode {
    /// The other mode (the `ManageDevices` toggle).
    pub fn toggled(self) -> Self {
        match self {
            Self::ReadWrite => Self::ReadOnly,
            Self::ReadOnly => Self::ReadWrite,
        }
    }

    /// Code stored in `shares.cfg`.
    pub fn code(self) -> &'static str {
        match self {
            Self::ReadWrite => "rw",
            Self::ReadOnly => "ro",
        }
    }

    /// Parses a `shares.cfg` code; anything unknown (or missing) is `ReadWrite`.
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "ro" => Self::ReadOnly,
            _ => Self::ReadWrite,
        }
    }
}

/// Syncthing shared folder configuration on PS Vita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareConfigEntry {
    pub folder_id: String,
    pub label: String,
    pub target_path: String,
    pub enabled: bool,
    /// Whether peers may modify this folder on the PS Vita.
    pub mode: ShareMode,
    /// Hex strings (`DeviceId::to_hex()`) of remote devices this folder is shared with.
    pub shared_devices: Vec<String>,
}

/// Folder a remote device shares with this PS Vita (received in its `ClusterConfig`),
/// waiting for the user's consent before it becomes a share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderOffer {
    pub folder_id: String,
    pub label: String,
    pub device_id: DeviceId,
    /// Postponed with `Later`: kept in the list but not prompted again until the app restarts.
    pub snoozed: bool,
}

/// Folder offer declined by the user: never prompted again for this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredFolder {
    pub folder_id: String,
    pub device_id: DeviceId,
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
    pub label: UiText,
    pub path: String,
    pub is_preset: bool,
    /// True if another share already targets this folder: shown greyed out and not selectable.
    pub is_shared: bool,
}

pub use crate::domain::defaults::NEW_SHARE_TEMPLATES;

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
