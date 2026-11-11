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

/// Renders Tab 4: **Settings & Log** (`Settings`).
pub fn render_settings_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    draw_page_title(gfx, th, strings::SETTINGS_TITLE, Some(strings::SETTINGS_SUBTITLE));

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
        strings::SETTINGS_SEC_DISPLAY,
    );

    let theme_desc = match state.theme_mode {
        ThemeMode::Dark => strings::SETTINGS_DESC_THEME_DARK,
        ThemeMode::Oled => strings::SETTINGS_DESC_THEME_OLED,
    };
    gfx.text(
        lx,
        CONTENT_Y + 52,
        16,
        FontWeight::Medium,
        th.text,
        Align::Left,
        strings::SETTINGS_LABEL_COLOUR_THEME,
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

    let w_theme = action_width(gfx, PadButton::Cross, strings::ACTION_SWITCH_THEME);
    let w_qr = action_width(gfx, PadButton::Circle, strings::ACTION_QR_ID);
    let w_add = action_width(gfx, PadButton::Triangle, strings::ACTION_ADD_DEVICE);
    let bx_theme = CONTENT_R - 20 - w_theme;
    let bx_qr = bx_theme - 8 - w_qr;
    let bx_add = bx_qr - 8 - w_add;
    draw_action(
        gfx,
        th,
        bx_add,
        CONTENT_Y + 62,
        PadButton::Triangle,
        strings::ACTION_ADD_DEVICE,
    );
    draw_action(gfx, th, bx_qr, CONTENT_Y + 62, PadButton::Circle, strings::ACTION_QR_ID);
    draw_action(
        gfx,
        th,
        bx_theme,
        CONTENT_Y + 62,
        PadButton::Cross,
        strings::ACTION_SWITCH_THEME,
    );
    let display_focus = state
        .focused_action(ActiveTab::Settings)
        .filter(|_| state.is_settings_row_focused(0));
    match display_focus {
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
        strings::SETTINGS_SAVER_AUTO_HINT,
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
        strings::SETTINGS_SEC_LOG,
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
            (PadButton::Dpad, Some(PadButton::Cross), strings::HINT_NAVIGATE),
            (PadButton::Circle, None, strings::ACTION_QR_ID),
            (PadButton::Triangle, None, strings::ACTION_ADD_DEVICE_ID),
            (PadButton::Select, None, strings::HINT_SCREENSAVER),
            (PadButton::Start, None, strings::ACTION_QUIT),
        ],
    );
}

/// Card listing folder offers declined in the `FolderOffer` modal, with a `Clear Declined` button.
fn render_declined_folders(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
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
        strings::SETTINGS_SEC_DECLINED,
    );
    let summary = if state.ignored_folders.is_empty() {
        strings::SETTINGS_NO_DECLINED.to_owned()
    } else {
        let ids: Vec<&str> = state
            .ignored_folders
            .iter()
            .map(|f| f.folder_id.as_str())
            .collect();
        strings::fmt_declined_summary(ids.len(), &ids.join(", "))
    };
    let label = strings::ACTION_CLEAR_DECLINED;
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
