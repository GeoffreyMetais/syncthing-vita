//! Layout geometry constants and Dark / OLED colour palettes (`UiTheme`).

use crate::domain::ThemeMode;
use crate::ui::gfx::{rgb, rgba, Color};

pub const SIDEBAR_W: i32 = 212;
pub const CONTENT_X: i32 = 236;
pub const CONTENT_R: i32 = 936;
pub const CONTENT_W: i32 = CONTENT_R - CONTENT_X;
pub const CONTENT_Y: i32 = 86;
pub const CONTENT_B: i32 = 488;
pub const HINT_Y: i32 = 500;

pub const BROWSER_TOP: i32 = 134;
pub const BROWSER_STEP: i32 = 42;

/// Top / bottom of the `Devices & Pairing` card on the Settings page.
pub const SETTINGS_DEVICES_Y: i32 = CONTENT_Y + 118;
pub const SETTINGS_DEVICES_B: i32 = SETTINGS_DEVICES_Y + 52;
/// Top / bottom of the `Language` card on the Settings page (tapping it cycles the language).
pub const SETTINGS_LANGUAGE_Y: i32 = SETTINGS_DEVICES_B + 8;
pub const SETTINGS_LANGUAGE_B: i32 = SETTINGS_LANGUAGE_Y + 50;
/// Top / bottom of the `Declined folders` card on the Settings page.
pub const SETTINGS_DECLINED_Y: i32 = SETTINGS_LANGUAGE_B + 8;
pub const SETTINGS_DECLINED_B: i32 = SETTINGS_DECLINED_Y + 56;

/// Size of the `ManageDevices` modal (wide enough for its five bottom actions).
pub const MANAGE_DEVICES_W: i32 = 780;
pub const MANAGE_DEVICES_H: i32 = 390;

/// Size of the `ConfirmQuit` modal (wide enough for the longest translated line).
pub const CONFIRM_QUIT_W: i32 = 720;
pub const CONFIRM_QUIT_H: i32 = 236;

/// Size of the camera viewfinder in the `ScanDeviceQr` modal (`640x360` frames scaled by 3/4).
pub const SCAN_QR_VIEW_W: i32 = 480;
pub const SCAN_QR_VIEW_H: i32 = 270;

/// `(x, y, w, h)` of the `ScanDeviceQr` modal, centred above the hint bar.
pub const fn scan_qr_modal_rect() -> (usize, usize, usize, usize) {
    let w = SCAN_QR_VIEW_W as usize + 56;
    let h = SCAN_QR_VIEW_H as usize + 140;
    ((960 - w) / 2, (HINT_Y as usize - h) / 2, w, h)
}

/// `(x, y, w, h)` of the `FolderOffer` modal, centred above the hint bar.
pub const fn folder_offer_modal_rect() -> (usize, usize, usize, usize) {
    let w = 620;
    let h = 260;
    ((960 - w) / 2, (HINT_Y as usize - h) / 2, w, h)
}

/// Touch width of each bottom action slot (`Decline`, `Later`, `Accept`) of the `FolderOffer` modal.
pub const FOLDER_OFFER_SLOT_W: usize = 160;

/// Left `x` of bottom action slot `slot` (0 = `Decline`, 1 = `Later`, 2 = `Accept`) of the
/// `FolderOffer` modal; the vertical centre of the action row is `y + h - 34`.
pub const fn folder_offer_slot_x(slot: usize) -> usize {
    let (dx, _, w, _) = folder_offer_modal_rect();
    dx + w - 28 - (3 - slot) * FOLDER_OFFER_SLOT_W - (2 - slot) * 12
}

/// Height of the `AddDeviceById` modal, which grows when the fallback keypad is shown.
pub const fn add_device_modal_height(keypad_visible: bool) -> i32 {
    if keypad_visible {
        424
    } else {
        256
    }
}

/// Height of the touch target of an action pill (the pill itself is 34 px tall).
pub const ACTION_TOUCH_H: i32 = 40;

/// Horizontal margin added around a hint of the bottom bar to form its touch target.
pub const HINT_TOUCH_MARGIN: i32 = 8;

pub const C_GOOD: Color = rgb(102, 187, 106);
pub const C_WARN: Color = rgb(255, 193, 77);
pub const C_BAD: Color = rgb(239, 83, 80);
/// Track (unfilled part) of the progress bars.
pub const C_PROGRESS_TRACK: Color = rgb(38, 47, 56);

/// Colour palette and corner radii for the VitaSend & Syncthing interfaces.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct UiTheme {
    pub bg: Color,
    pub sidebar: Color,
    pub card: Color,
    pub card_hi: Color,
    pub btn: Color,
    pub border: Color,
    pub track: Color,
    pub toast: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub hint_text: Color,
    pub dpad_on: Color,
    pub dpad_off: Color,
    pub accent: Color,
    pub accent_hi: Color,
    pub accent_bg: Color,
    pub on_accent: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub divider: bool,
    pub card_radius: i32,
    pub button_radius: i32,
    pub modal_radius: i32,
}

impl UiTheme {
    pub const fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Oled => Self::oled(),
            ThemeMode::SyncthingDark => Self::syncthing_dark(),
            ThemeMode::SyncthingLight => Self::syncthing_light(),
        }
    }

    pub const fn dark() -> Self {
        Self {
            bg: rgb(15, 19, 23),
            sidebar: rgb(11, 14, 17),
            card: rgb(25, 31, 37),
            card_hi: rgb(31, 40, 48),
            btn: rgb(36, 45, 54),
            border: rgb(40, 49, 58),
            track: C_PROGRESS_TRACK,
            toast: rgb(54, 63, 74),
            text: rgb(232, 238, 243),
            dim: rgb(140, 152, 164),
            faint: rgb(88, 99, 110),
            hint_text: rgb(176, 186, 198),
            dpad_on: rgb(214, 222, 234),
            dpad_off: rgb(84, 94, 110),
            accent: rgb(38, 166, 154),
            accent_hi: rgb(72, 208, 190),
            accent_bg: rgba(38, 166, 154, 46),
            on_accent: rgb(6, 24, 22),
            good: C_GOOD,
            warn: C_WARN,
            bad: C_BAD,
            divider: false,
            card_radius: 14,
            button_radius: 17,
            modal_radius: 18,
        }
    }

    pub const fn oled() -> Self {
        Self {
            bg: rgb(0, 0, 0),
            sidebar: rgb(0, 0, 0),
            card: rgb(15, 15, 16),
            card_hi: rgb(26, 27, 28),
            btn: rgb(28, 30, 34),
            border: rgb(38, 40, 42),
            track: rgb(30, 32, 36),
            toast: rgb(42, 44, 48),
            text: rgb(240, 242, 244),
            dim: rgb(150, 156, 162),
            faint: rgb(96, 101, 106),
            hint_text: rgb(176, 186, 198),
            dpad_on: rgb(214, 222, 234),
            dpad_off: rgb(84, 94, 110),
            accent: rgb(38, 166, 154),
            accent_hi: rgb(72, 208, 190),
            accent_bg: rgba(38, 166, 154, 46),
            on_accent: rgb(2, 20, 18),
            good: C_GOOD,
            warn: C_WARN,
            bad: C_BAD,
            divider: true,
            card_radius: 14,
            button_radius: 17,
            modal_radius: 18,
        }
    }

    /// Official Syncthing dark Web UI palette with subtle Bootstrap-style corner radii.
    pub const fn syncthing_dark() -> Self {
        Self {
            bg: rgb(39, 39, 39),
            sidebar: rgb(51, 51, 51),
            card: rgb(59, 59, 59),
            card_hi: rgb(69, 69, 69),
            btn: rgb(51, 51, 51),
            border: rgb(78, 78, 78),
            track: rgb(45, 45, 45),
            toast: rgb(64, 64, 64),
            text: rgb(224, 224, 224),
            dim: rgb(160, 160, 160),
            faint: rgb(115, 115, 115),
            hint_text: rgb(190, 190, 190),
            dpad_on: rgb(214, 222, 234),
            dpad_off: rgb(84, 94, 110),
            accent: rgb(18, 148, 206),
            accent_hi: rgb(56, 182, 236),
            accent_bg: rgba(18, 148, 206, 46),
            on_accent: rgb(255, 255, 255),
            good: rgb(92, 184, 92),
            warn: rgb(240, 173, 78),
            bad: rgb(217, 83, 79),
            divider: true,
            card_radius: 6,
            button_radius: 5,
            modal_radius: 8,
        }
    }

    /// Official Syncthing light Web UI palette with subtle Bootstrap-style corner radii.
    pub const fn syncthing_light() -> Self {
        Self {
            bg: rgb(245, 245, 245),
            sidebar: rgb(238, 238, 238),
            card: rgb(255, 255, 255),
            card_hi: rgb(247, 250, 253),
            btn: rgb(255, 255, 255),
            border: rgb(208, 212, 217),
            track: rgb(224, 228, 232),
            toast: rgb(255, 255, 255),
            text: rgb(51, 51, 51),
            dim: rgb(102, 102, 102),
            faint: rgb(148, 154, 160),
            hint_text: rgb(72, 76, 82),
            dpad_on: rgb(58, 66, 76),
            dpad_off: rgb(168, 174, 182),
            accent: rgb(17, 125, 176),
            accent_hi: rgb(8, 108, 162),
            accent_bg: rgba(17, 125, 176, 30),
            on_accent: rgb(255, 255, 255),
            good: rgb(46, 139, 60),
            warn: rgb(192, 114, 0),
            bad: rgb(201, 48, 44),
            divider: true,
            card_radius: 6,
            button_radius: 5,
            modal_radius: 8,
        }
    }
}
