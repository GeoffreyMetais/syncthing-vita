//! Tab 4 (`Settings & Log` / `Settings`): Dark / Pure-Black OLED theme switcher,
//! `QR / ID` modal shortcut, and BEP/1.0 & TLS 1.3 protocol event log.

use crate::domain::{ActiveTab, AppState, ThemeMode};
use crate::ui::gfx::{Align, FontWeight, Gfx, PadButton};
use crate::ui::theme::{
    UiTheme, CONTENT_B, CONTENT_R, CONTENT_W, CONTENT_X, CONTENT_Y, C_BAD, C_GOOD,
};
use crate::ui::widgets::{
    action_width, draw_action, draw_card, draw_focus_ring, draw_hints, draw_page_title,
};

/// Renders Tab 4: **Settings & Log** (`Settings`).
pub fn render_settings_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    draw_page_title(gfx, th, "Settings & Log", Some("Display & BEP/1.0"));

    // 1. Display & Theme Settings Card
    draw_card(gfx, th, CONTENT_X, CONTENT_Y, CONTENT_W, 118, true);
    let lx = CONTENT_X + 24;
    gfx.text(
        lx,
        CONTENT_Y + 22,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        "DISPLAY AND OLED SAVER",
    );

    let theme_desc = match state.theme_mode {
        ThemeMode::Dark => "Dark — VitaSend slate blue-grey look with teal accent",
        ThemeMode::Oled => "OLED — Pure #000000 black background (saves PS Vita OLED battery)",
    };
    gfx.text(
        lx,
        CONTENT_Y + 52,
        16,
        FontWeight::Medium,
        th.text,
        Align::Left,
        "Colour Theme",
    );
    gfx.text_fit(
        lx,
        CONTENT_Y + 76,
        CONTENT_W - 220,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        theme_desc,
    );

    let w_theme = action_width(gfx, PadButton::Cross, "Switch Theme");
    let w_qr = action_width(gfx, PadButton::Circle, "QR / ID");
    let w_add = action_width(gfx, PadButton::Triangle, "+ Add Device");
    let bx_theme = CONTENT_R - 20 - w_theme;
    let bx_qr = bx_theme - 8 - w_qr;
    let bx_add = bx_qr - 8 - w_add;
    draw_action(
        gfx,
        th,
        bx_add,
        CONTENT_Y + 62,
        PadButton::Triangle,
        "+ Add Device",
    );
    draw_action(gfx, th, bx_qr, CONTENT_Y + 62, PadButton::Circle, "QR / ID");
    draw_action(
        gfx,
        th,
        bx_theme,
        CONTENT_Y + 62,
        PadButton::Cross,
        "Switch Theme",
    );
    match state.focused_action(ActiveTab::Settings) {
        Some(0) => draw_focus_ring(gfx, th, bx_add, CONTENT_Y + 62, w_add),
        Some(1) => draw_focus_ring(gfx, th, bx_qr, CONTENT_Y + 62, w_qr),
        Some(_) => draw_focus_ring(gfx, th, bx_theme, CONTENT_Y + 62, w_theme),
        None => {}
    }

    gfx.text(
        lx,
        CONTENT_Y + 102,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        "OLED Screensaver activates automatically after 12s during sync (or press SELECT anytime).",
    );

    // 2. Full Protocol Event Log Card
    let ly = CONTENT_Y + 132;
    let lh = CONTENT_B - ly;
    draw_card(gfx, th, CONTENT_X, ly, CONTENT_W, lh, false);
    gfx.text(
        lx,
        ly + 22,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        "BEP/1.0 & TLS 1.3 PROTOCOL LOG",
    );

    for (i, line) in state.logs.iter().rev().take(10).enumerate() {
        let y = ly + 48 + (i as i32) * 22;
        let col = if line.contains("Error") || line.contains("Failed") {
            C_BAD
        } else if line.contains("Synced")
            || line.contains("Connected")
            || line.contains("Sync OK")
            || line.contains("up-to-date")
        {
            C_GOOD
        } else {
            th.dim
        };
        gfx.text_fit(
            lx,
            y,
            CONTENT_W - 48,
            13,
            FontWeight::Regular,
            col,
            Align::Left,
            line,
        );
    }

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Dpad, Some(PadButton::Cross), "Navigate"),
            (PadButton::Circle, None, "Device ID / QR"),
            (PadButton::Triangle, None, "+ Add Device ID"),
            (PadButton::Select, None, "Screensaver"),
            (PadButton::Start, None, "Quit"),
        ],
    );
}
