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
/// Touch width of the "Open" chip at the right of a directory row in the `Files` browser.
pub const OPEN_CHIP_W: i32 = 120;

pub const C_GOOD: Color = rgb(102, 187, 106);
pub const C_WARN: Color = rgb(255, 193, 77);
pub const C_BAD: Color = rgb(239, 83, 80);

/// Colour palette for the VitaSend-inspired interface.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct UiTheme {
    pub bg: Color,
    pub sidebar: Color,
    pub card: Color,
    pub card_hi: Color,
    pub border: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent_hi: Color,
    pub accent_bg: Color,
    pub on_accent: Color,
    pub divider: bool,
}

impl UiTheme {
    pub const fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Oled => Self::oled(),
        }
    }

    pub const fn dark() -> Self {
        Self {
            bg: rgb(15, 19, 23),
            sidebar: rgb(11, 14, 17),
            card: rgb(25, 31, 37),
            card_hi: rgb(31, 40, 48),
            border: rgb(40, 49, 58),
            text: rgb(232, 238, 243),
            dim: rgb(140, 152, 164),
            faint: rgb(88, 99, 110),
            accent: rgb(38, 166, 154),
            accent_hi: rgb(72, 208, 190),
            accent_bg: rgba(38, 166, 154, 46),
            on_accent: rgb(6, 24, 22),
            divider: false,
        }
    }

    pub const fn oled() -> Self {
        Self {
            bg: rgb(0, 0, 0),
            sidebar: rgb(0, 0, 0),
            card: rgb(15, 15, 16),
            card_hi: rgb(26, 27, 28),
            border: rgb(38, 40, 42),
            text: rgb(240, 242, 244),
            dim: rgb(150, 156, 162),
            faint: rgb(96, 101, 106),
            accent: rgb(38, 166, 154),
            accent_hi: rgb(72, 208, 190),
            accent_bg: rgba(38, 166, 154, 46),
            on_accent: rgb(2, 20, 18),
            divider: true,
        }
    }
}
