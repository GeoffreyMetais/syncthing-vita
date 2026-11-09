//! On-screen fallback Base32 keypad component for manual device ID entry.
//! Used when the native PS Vita System Keyboard (`SceIme`) is closed or as a touch/gamepad fallback.

use crate::domain::ADD_DEVICE_KEYPAD;
use crate::ui::gfx::{rgba, Align, Color, FontWeight, Gfx};
use crate::ui::theme::{UiTheme, C_GOOD};

/// Grid layout constants for the 36-key (9x4) Base32 keypad.
pub const KEYPAD_COLS: usize = 9;
pub const KEYPAD_CELL_W: i32 = 68;
pub const KEYPAD_CELL_H: i32 = 38;
pub const KEYPAD_GAP: i32 = 4;
pub const KEYPAD_STEP_X: i32 = KEYPAD_CELL_W + KEYPAD_GAP; // 72
pub const KEYPAD_STEP_Y: i32 = KEYPAD_CELL_H + KEYPAD_GAP; // 42

/// Returns the color palette `(bg, fg, outline)` for a keypad button.
pub fn keypad_button_colors(
    th: &UiTheme,
    label: &str,
    is_sel: bool,
) -> (Color, Color, Color) {
    let is_add = label == "Add";
    let is_action = label == "Bksp" || label == "Clear";

    let bg = if is_sel {
        th.accent
    } else if is_add {
        rgba(38, 166, 154, 48)
    } else if is_action {
        rgba(255, 255, 255, 16)
    } else {
        th.bg
    };

    let fg = if is_sel {
        th.on_accent
    } else if is_add {
        C_GOOD
    } else if is_action {
        th.accent_hi
    } else {
        th.text
    };

    let outline = if is_sel { th.accent_hi } else { th.border };

    (bg, fg, outline)
}

/// Renders the 9x4 Quick Base32 Keypad on screen.
pub fn draw_fallback_keypad(
    gfx: &mut Gfx,
    th: &UiTheme,
    origin_x: i32,
    grid_y: i32,
    selected_idx: usize,
    ime_active: bool,
) {
    for (idx, &label) in ADD_DEVICE_KEYPAD.iter().enumerate() {
        let col = (idx % KEYPAD_COLS) as i32;
        let row = (idx / KEYPAD_COLS) as i32;
        let kx = origin_x + col * KEYPAD_STEP_X;
        let ky = grid_y + row * KEYPAD_STEP_Y;
        let is_sel = idx == selected_idx && !ime_active;

        let (bg, fg, outline) = keypad_button_colors(th, label, is_sel);

        gfx.rrect(kx, ky, KEYPAD_CELL_W, KEYPAD_CELL_H, 8, bg);
        gfx.rrect_outline(kx, ky, KEYPAD_CELL_W, KEYPAD_CELL_H, 8, 1, outline);
        gfx.text(
            kx + KEYPAD_CELL_W / 2,
            ky + KEYPAD_CELL_H / 2,
            if label.len() > 1 { 13 } else { 15 },
            FontWeight::Bold,
            fg,
            Align::Center,
            label,
        );
    }
}
