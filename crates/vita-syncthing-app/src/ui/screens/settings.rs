//! Tab 4 (`Settings & Log` / `Settings`): Dark / Pure-Black OLED theme switcher,
//! `QR / ID` modal shortcut, declined folder offers (`Clear Declined`), and BEP/1.0 & TLS 1.3
//! protocol event log.

use crate::domain::{ActiveTab, AppState, ThemeMode};
use crate::ui::gfx::{Align, FontWeight, Gfx, PadButton};
use crate::ui::strings;
use crate::ui::theme::{
    UiTheme, CONTENT_B, CONTENT_R, CONTENT_W, CONTENT_X, CONTENT_Y, C_BAD, C_GOOD,
    SETTINGS_DECLINED_B, SETTINGS_DECLINED_Y,
};
use crate::ui::widgets::{
    action_width, draw_action, draw_card, draw_focus_ring, draw_hints, draw_page_title,
};

/// Vertical centre of the theme card actions: beside the short "Colour Theme"
/// label, between the section header and the full-width description line.
const THEME_ACTIONS_Y: i32 = CONTENT_Y + 48;

/// Renders Tab 4: **Settings & Log** (`Settings`).
pub fn render_settings_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let s = strings::strings(state.language);
    draw_page_title(gfx, th, s.settings_title, Some(s.settings_subtitle));

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
        s.settings_sec_display,
    );

    let theme_desc = match state.theme_mode {
        ThemeMode::Dark => s.settings_desc_theme_dark,
        ThemeMode::Oled => s.settings_desc_theme_oled,
    };
    gfx.text(
        lx,
        CONTENT_Y + 52,
        16,
        FontWeight::Medium,
        th.text,
        Align::Left,
        s.settings_label_colour_theme,
    );
    let w_theme = action_width(gfx, PadButton::Cross, s.action_switch_theme);
    let w_qr = action_width(gfx, PadButton::Circle, s.action_qr_id);
    let w_add = action_width(gfx, PadButton::Triangle, s.action_add_device);
    let bx_theme = CONTENT_R - 20 - w_theme;
    let bx_qr = bx_theme - 8 - w_qr;
    let bx_add = bx_qr - 8 - w_add;

    // The actions end above this line, so the description gets the full card width.
    gfx.text_fit(
        lx,
        CONTENT_Y + 76,
        CONTENT_R - 24 - lx,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        theme_desc,
    );
    draw_action(
        gfx,
        th,
        bx_add,
        THEME_ACTIONS_Y,
        PadButton::Triangle,
        s.action_add_device,
    );
    draw_action(
        gfx,
        th,
        bx_qr,
        THEME_ACTIONS_Y,
        PadButton::Circle,
        s.action_qr_id,
    );
    draw_action(
        gfx,
        th,
        bx_theme,
        THEME_ACTIONS_Y,
        PadButton::Cross,
        s.action_switch_theme,
    );
    let display_focus = state
        .focused_action(ActiveTab::Settings)
        .filter(|_| state.is_settings_row_focused(0));
    match display_focus {
        Some(0) => draw_focus_ring(gfx, th, bx_add, THEME_ACTIONS_Y, w_add),
        Some(1) => draw_focus_ring(gfx, th, bx_qr, THEME_ACTIONS_Y, w_qr),
        Some(_) => draw_focus_ring(gfx, th, bx_theme, THEME_ACTIONS_Y, w_theme),
        None => {}
    }

    gfx.text(
        lx,
        CONTENT_Y + 102,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        s.settings_saver_auto_hint,
    );

    // 2. Declined Folder Offers Card
    render_declined_folders(gfx, th, state);

    // 3. Full Protocol Event Log Card
    let ly = SETTINGS_DECLINED_B + 14;
    let lh = CONTENT_B - ly;
    draw_card(gfx, th, CONTENT_X, ly, CONTENT_W, lh, false);
    gfx.text(
        lx,
        ly + 22,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.settings_sec_log,
    );

    let max_lines = ((lh - 56) / 22).max(0) as usize;
    for (i, line) in state.logs.iter().rev().take(max_lines).enumerate() {
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
            (PadButton::Dpad, Some(PadButton::Cross), s.hint_navigate),
            (PadButton::Circle, None, s.action_qr_id),
            (PadButton::Triangle, None, s.action_add_device_id),
            (PadButton::Select, None, s.hint_screensaver),
            (PadButton::Start, None, s.action_quit),
        ],
    );
}

/// Card listing folder offers declined in the `FolderOffer` modal, with a `Clear Declined` button.
fn render_declined_folders(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let s = strings::strings(state.language);
    let y = SETTINGS_DECLINED_Y;
    let h = SETTINGS_DECLINED_B - SETTINGS_DECLINED_Y;
    draw_card(gfx, th, CONTENT_X, y, CONTENT_W, h, false);
    let lx = CONTENT_X + 24;
    gfx.text(
        lx,
        y + 22,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.settings_sec_declined,
    );
    let summary = if state.ignored_folders.is_empty() {
        s.settings_no_declined.to_owned()
    } else {
        let ids: Vec<&str> = state
            .ignored_folders
            .iter()
            .map(|f| f.folder_id.as_str())
            .collect();
        s.fmt_declined_summary(ids.len(), &ids.join(", "))
    };
    let label = s.action_clear_declined;
    let w_clear = action_width(gfx, PadButton::Cross, label);
    let bx = CONTENT_R - 20 - w_clear;
    let cy = y + h / 2;
    let col = if state.ignored_folders.is_empty() {
        th.faint
    } else {
        th.dim
    };
    gfx.text_fit(
        lx,
        y + 48,
        bx - lx - 16,
        14,
        FontWeight::Regular,
        col,
        Align::Left,
        &summary,
    );
    draw_action(gfx, th, bx, cy, PadButton::Cross, label);
    if state.is_settings_row_focused(1) {
        draw_focus_ring(gfx, th, bx, cy, w_clear);
    }
}
