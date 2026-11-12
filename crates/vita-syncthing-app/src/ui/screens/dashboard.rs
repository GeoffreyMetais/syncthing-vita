//! Tab 1 (`Overview` / `Dashboard`): Device status hero card, `QR / ID` & `Sync Now` actions,
//! transfer statistics, and recent synchronization activity feed.

use crate::domain::{fmt_size, AppState, FocusSlot, HitMap, Hotspot, Rect, UiAction, UiText};
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::strings;
use crate::ui::theme::{
    UiTheme, CONTENT_B, CONTENT_R, CONTENT_W, CONTENT_X, CONTENT_Y, C_BAD, C_GOOD, C_WARN,
};
use crate::ui::widgets::{
    action_width, draw_action_hotspot, draw_card, draw_chip, draw_hint_bar, draw_page_title, Hint,
};

/// Height of the "This device" hero card.
const HERO_CARD_H: i32 = 152;

/// Renders Tab 1: **Overview** (`Dashboard`).
pub fn render_dashboard_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let active_count = state.shares.iter().filter(|s| s.enabled).count();
    let subtitle = s.fmt_active_shares_devices(active_count, state.devices.len());
    draw_page_title(gfx, th, s.dashboard_title, Some(&subtitle));

    // 1. "This device" hero card
    draw_card(gfx, th, CONTENT_X, CONTENT_Y, CONTENT_W, HERO_CARD_H, true);
    // Touching the device card shows its ID; the buttons declared below sit on top of it.
    hits.add(Hotspot {
        rect: Rect::new(CONTENT_X, CONTENT_Y, CONTENT_W, HERO_CARD_H),
        action: UiAction::ShowDeviceId,
        focus: None,
        shortcut: None,
    });
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
        (s.status_syncing, th.accent_hi)
    } else if state.status_line == UiText::ScanningLan {
        (s.status_scanning, C_WARN)
    } else {
        (s.status_ready, C_GOOD)
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
        s.hero_this_device,
    );
    gfx.text_fit(
        tx,
        CONTENT_Y + 52,
        avail,
        24,
        FontWeight::Bold,
        th.text,
        Align::Left,
        s.hero_device_name,
    );

    let peer_str = s.fmt_peer_and_folders(&s.text(&state.peer_label), &s.text(&state.folder_label));
    gfx.text_fit(
        tx,
        CONTENT_Y + 77,
        CONTENT_R - 24 - tx,
        15,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &peer_str,
    );

    // Kept clear of the focus ring around the actions below (`CONTENT_Y + 128 ± 21`).
    gfx.text_fit(
        tx,
        CONTENT_Y + 98,
        CONTENT_R - 24 - tx,
        15,
        FontWeight::Medium,
        th.accent_hi,
        Align::Left,
        &s.text(&state.status_line),
    );

    let act_sync_w = action_width(gfx, PadButton::Cross, s.action_sync_now);
    let act_qr_w = action_width(gfx, PadButton::Circle, s.action_qr_id);
    let bx_sync = CONTENT_R - 18 - act_sync_w;
    let bx_qr = bx_sync - 10 - act_qr_w;

    // The ID value follows the (translated) label instead of a fixed column.
    let id_label_w = gfx.text(
        lx,
        CONTENT_Y + 134,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        s.hero_device_id_label,
    );
    let id_x = lx + id_label_w.max(72) + 10;
    gfx.text_fit(
        id_x,
        CONTENT_Y + 134,
        bx_qr - 14 - id_x,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &state.vita_device_id,
    );

    let buttons = [
        (bx_qr, PadButton::Circle, s.action_qr_id, UiAction::ShowDeviceId),
        (bx_sync, PadButton::Cross, s.action_sync_now, UiAction::StartSync),
    ];
    for (col, (bx, pad, label, action)) in buttons.into_iter().enumerate() {
        let slot = FocusSlot::RowAction { row: 0, col };
        draw_action_hotspot(
            gfx,
            th,
            hits,
            bx,
            CONTENT_Y + 128,
            pad,
            label,
            action,
            slot,
            state.is_focused(slot),
        );
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
        s.section_recent_activity,
    );

    let stats_chip = s.fmt_recent_stats(
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
            s.msg_no_sync_events,
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

    draw_hint_bar(
        gfx,
        th,
        hits,
        &[
            Hint::info(PadButton::Dpad, Some(PadButton::Cross), s.hint_navigate),
            Hint::action(PadButton::Circle, s.action_qr_id, UiAction::ShowDeviceId),
            Hint::action(PadButton::Square, s.action_pause, UiAction::ToggleSelectedShare),
            Hint::action(
                PadButton::Triangle,
                s.hint_folder,
                UiAction::ChangeSelectedShareFolder,
            ),
            Hint::info(PadButton::L, Some(PadButton::R), s.hint_tabs),
            Hint::action(PadButton::Select, s.hint_screensaver, UiAction::StartScreensaver),
        ],
    );
}
