//! Tab 1 (`Overview` / `Dashboard`): Device status hero card, `QR / ID` & `Sync Now` actions,
//! transfer statistics, and recent synchronization activity feed.

use crate::domain::{fmt_size, ActiveTab, AppState};
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::strings;
use crate::ui::theme::{
    UiTheme, CONTENT_B, CONTENT_R, CONTENT_W, CONTENT_X, CONTENT_Y, C_BAD, C_GOOD, C_WARN,
};
use crate::ui::widgets::{
    action_width, draw_action, draw_card, draw_chip, draw_focus_ring, draw_hints, draw_page_title,
};

/// Renders Tab 1: **Overview** (`Dashboard`).
pub fn render_dashboard_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let active_count = state.shares.iter().filter(|s| s.enabled).count();
    let subtitle = strings::fmt_active_shares_devices(active_count, state.devices.len());
    draw_page_title(gfx, th, strings::DASHBOARD_TITLE, Some(&subtitle));

    // 1. "This device" hero card
    draw_card(gfx, th, CONTENT_X, CONTENT_Y, CONTENT_W, 152, true);
    let lx = CONTENT_X + 24;
    gfx.rrect(lx, CONTENT_Y + 24, 76, 76, 20, th.accent_bg);
    gfx.icon(
        Icon::Vita,
        (lx + 38) as f32,
        (CONTENT_Y + 62) as f32,
        60.0,
        th.accent_hi,
    );

    let tx = lx + 100;
    let (chip_txt, chip_col) = if state.sync_in_progress {
        (strings::STATUS_SYNCING, th.accent_hi)
    } else if state.peer_label.contains("Scanning") {
        (strings::STATUS_SCANNING, C_WARN)
    } else {
        (strings::STATUS_READY, C_GOOD)
    };
    let cw = draw_chip(gfx, CONTENT_R - 22, CONTENT_Y + 26, chip_txt, chip_col);
    let avail = CONTENT_R - 24 - tx - cw;

    gfx.text(
        tx,
        CONTENT_Y + 24,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        strings::HERO_THIS_DEVICE,
    );
    gfx.text_fit(
        tx,
        CONTENT_Y + 52,
        avail,
        24,
        FontWeight::Bold,
        th.text,
        Align::Left,
        strings::HERO_DEVICE_NAME,
    );

    let peer_str = strings::fmt_peer_and_folders(&state.peer_label, &state.folder_label);
    gfx.text_fit(
        tx,
        CONTENT_Y + 80,
        CONTENT_R - 24 - tx,
        15,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &peer_str,
    );

    gfx.text_fit(
        tx,
        CONTENT_Y + 104,
        CONTENT_R - 24 - tx,
        15,
        FontWeight::Medium,
        th.accent_hi,
        Align::Left,
        &state.status_line,
    );

    let act_sync_w = action_width(gfx, PadButton::Cross, strings::ACTION_SYNC_NOW);
    let act_qr_w = action_width(gfx, PadButton::Circle, strings::ACTION_QR_ID);
    let bx_sync = CONTENT_R - 18 - act_sync_w;
    let bx_qr = bx_sync - 10 - act_qr_w;

    gfx.text(
        lx,
        CONTENT_Y + 134,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        strings::HERO_DEVICE_ID_LABEL,
    );
    gfx.text_fit(
        lx + 82,
        CONTENT_Y + 134,
        bx_qr - 14 - (lx + 82),
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &state.vita_device_id,
    );

    draw_action(
        gfx,
        th,
        bx_qr,
        CONTENT_Y + 128,
        PadButton::Circle,
        strings::ACTION_QR_ID,
    );
    draw_action(
        gfx,
        th,
        bx_sync,
        CONTENT_Y + 128,
        PadButton::Cross,
        strings::ACTION_SYNC_NOW,
    );
    match state.focused_action(ActiveTab::Dashboard) {
        Some(0) => draw_focus_ring(gfx, th, bx_qr, CONTENT_Y + 128, act_qr_w),
        Some(_) => draw_focus_ring(gfx, th, bx_sync, CONTENT_Y + 128, act_sync_w),
        None => {}
    }

    // 2. "Recent activity & Statistics" card
    let ry = CONTENT_Y + 166;
    let rh = CONTENT_B - ry;
    draw_card(gfx, th, CONTENT_X, ry, CONTENT_W, rh, false);
    gfx.text(
        lx,
        ry + 24,
        16,
        FontWeight::Medium,
        th.text,
        Align::Left,
        strings::SECTION_RECENT_ACTIVITY,
    );

    let stats_chip = strings::fmt_recent_stats(
        state.files_up_to_date,
        state.files_downloaded,
        state.blocks_uploaded,
        &fmt_size(state.bytes_transferred),
    );
    draw_chip(gfx, CONTENT_R - 22, ry + 24, &stats_chip, th.accent_hi);

    let max_rows = 6usize;
    let start = state.logs.len().saturating_sub(max_rows);
    let recent = &state.logs[start..];

    if recent.is_empty() {
        gfx.text(
            lx,
            ry + 68,
            15,
            FontWeight::Regular,
            th.faint,
            Align::Left,
            strings::MSG_NO_SYNC_EVENTS,
        );
    } else {
        for (i, line) in recent.iter().rev().enumerate() {
            let cy = ry + 58 + (i as i32) * 30;
            let (icon, col) = if line.contains("Error") || line.contains("Failed") {
                (Icon::Close, C_BAD)
            } else if line.contains("Block uploaded") || line.contains("Local index") {
                (Icon::ArrowUp, th.accent_hi)
            } else if line.contains("Synced:") {
                (Icon::ArrowDown, C_GOOD)
            } else if line.contains("up-to-date") || line.contains("Sync OK") {
                (Icon::Check, C_GOOD)
            } else {
                (Icon::Refresh, th.dim)
            };

            gfx.circle((lx + 12) as f32, cy as f32, 12.0, rgba(255, 255, 255, 14));
            gfx.icon(icon, (lx + 12) as f32, cy as f32, 16.0, col);
            gfx.text_fit(
                lx + 36,
                cy,
                CONTENT_W - 84,
                14,
                FontWeight::Regular,
                if col == C_BAD { C_BAD } else { th.text },
                Align::Left,
                line,
            );
        }
    }

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Dpad, Some(PadButton::Cross), strings::HINT_NAVIGATE),
            (PadButton::Circle, None, strings::ACTION_QR_ID),
            (PadButton::Square, None, strings::ACTION_PAUSE),
            (PadButton::Triangle, None, strings::HINT_FOLDER),
            (PadButton::L, Some(PadButton::R), strings::HINT_TABS),
            (PadButton::Select, None, strings::HINT_SCREENSAVER),
        ],
    );
}
