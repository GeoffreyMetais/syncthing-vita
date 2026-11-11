//! Modal Dialog: Add or remove Syncthing devices for a share (`shares[share_idx]`).

use syncthing_core::DeviceId;

use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_GOOD, C_WARN, HINT_Y, MANAGE_DEVICES_H, MANAGE_DEVICES_W};
use crate::ui::widgets::{action_width, draw_action, draw_chip, draw_hints, draw_row_tick};

/// Modal Dialog 2: Add or remove Syncthing devices for a share (`shares[share_idx]`).
pub fn render_manage_devices_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
    selected_dev_idx: usize,
) {
    gfx.dim(175);
    let w = MANAGE_DEVICES_W;
    let h = MANAGE_DEVICES_H;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let share_title = state
        .shares
        .get(share_idx)
        .map(|s| format!("{} ({})", s.label, s.folder_id))
        .unwrap_or_else(|| strings::MODAL_MANAGE_DEVICES_DEFAULT_TITLE.to_owned());
    let shared_list = state
        .shares
        .get(share_idx)
        .map(|s| s.shared_devices.as_slice())
        .unwrap_or(&[]);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Desktop,
        (lx + 20) as f32,
        (dy + 40) as f32,
        22.0,
        th.accent_hi,
    );
    let header_title = strings::fmt_devices_for_share(&share_title);
    gfx.text_fit(
        lx + 52,
        dy + 32,
        w - 110,
        20,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &header_title,
    );
    let incomplete = state.is_share_creation_incomplete(share_idx);
    let (subtitle, subtitle_col) = if incomplete {
        (
            strings::MODAL_MANAGE_DEVICES_SUBTITLE_INCOMPLETE,
            C_WARN,
        )
    } else {
        (
            strings::MODAL_MANAGE_DEVICES_SUBTITLE,
            th.dim,
        )
    };
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        subtitle_col,
        Align::Left,
        subtitle,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    if state.devices.is_empty() {
        let (headline, col) = if state.sync_in_progress {
            (
                strings::MODAL_MANAGE_DEVICES_SCANNING,
                th.accent_hi,
            )
        } else {
            (strings::MODAL_MANAGE_DEVICES_EMPTY, C_WARN)
        };
        gfx.text(
            lx,
            dy + 130,
            16,
            FontWeight::Medium,
            col,
            Align::Left,
            headline,
        );
        let sub_hint = if state.sync_in_progress {
            state.status_line.as_str()
        } else {
            strings::MODAL_MANAGE_DEVICES_EMPTY_HINT
        };
        gfx.text(
            lx,
            dy + 160,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            sub_hint,
        );
    } else {
        for (i, dev) in state.devices.iter().take(5).enumerate() {
            let ry = dy + 92 + (i as i32) * 52;
            let cy = ry + 23;
            let is_sel = i == selected_dev_idx;
            let is_checked = shared_list
                .iter()
                .filter_map(|h| DeviceId::from_hex(h))
                .any(|id| id.matches_peer(&dev.device_id));

            if is_sel {
                gfx.rrect(lx, ry, w - 56, 46, 10, th.accent_bg);
                gfx.rrect(lx, ry + 10, 4, 26, 2, th.accent);
            }

            draw_row_tick(gfx, th, lx + 24, cy, is_checked);
            gfx.icon(
                Icon::Laptop,
                (lx + 54) as f32,
                cy as f32,
                22.0,
                if is_checked { th.accent_hi } else { th.dim },
            );

            let badge_txt = if is_checked { strings::ACTION_LINKED } else { strings::ACTION_NOT_LINKED };
            let badge_col = if is_checked { C_GOOD } else { th.dim };
            draw_chip(gfx, dx + w - 36, cy, badge_txt, badge_col);

            gfx.text(
                lx + 78,
                cy - 7,
                16,
                FontWeight::Bold,
                th.text,
                Align::Left,
                &dev.name,
            );
            let sub = if dev.device_id.is_short_placeholder() {
                format!(
                    "Short ID: {} (auto-resolves on LAN)  ·  Address: {}",
                    dev.device_id.syncthing_prefix(),
                    dev.address
                )
            } else {
                format!(
                    "ID: {}  ·  Address: {}",
                    dev.device_id.to_syncthing_string(),
                    dev.address
                )
            };
            gfx.text_fit(
                lx + 78,
                cy + 12,
                w - 240,
                12,
                FontWeight::Regular,
                th.dim,
                Align::Left,
                &sub,
            );
        }
    }

    // Bottom modal action buttons
    let pause_lbl = if state.shares.get(share_idx).is_some_and(|s| s.enabled) {
        strings::ACTION_PAUSE
    } else {
        strings::ACTION_ENABLE
    };
    draw_action(gfx, th, lx, dy + h - 32, PadButton::Select, pause_lbl);
    let w_add = action_width(gfx, PadButton::Square, strings::ACTION_ADD_BY_ID);
    let w_sync = action_width(gfx, PadButton::Triangle, strings::ACTION_APPLY_SYNC);
    let close_lbl = if incomplete { strings::ACTION_CANCEL } else { strings::ACTION_DONE };
    let w_close = action_width(gfx, PadButton::Circle, close_lbl);
    let bx_close = dx + w - 28 - w_close;
    let bx_sync = bx_close - 10 - w_sync;
    let bx_add = bx_sync - 10 - w_add;
    let w_scan = action_width(gfx, PadButton::R, strings::ACTION_SCAN_QR);
    let bx_scan = bx_add - 10 - w_scan;
    draw_action(gfx, th, bx_scan, dy + h - 32, PadButton::R, strings::ACTION_SCAN_QR);
    draw_action(
        gfx,
        th,
        bx_add,
        dy + h - 32,
        PadButton::Square,
        strings::ACTION_ADD_BY_ID,
    );
    draw_action(
        gfx,
        th,
        bx_sync,
        dy + h - 32,
        PadButton::Triangle,
        strings::ACTION_APPLY_SYNC,
    );
    draw_action(gfx, th, bx_close, dy + h - 32, PadButton::Circle, close_lbl);

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::DpadUd, None, strings::HINT_SELECT),
            (PadButton::Cross, None, strings::HINT_TOGGLE),
            (PadButton::Select, None, strings::HINT_PAUSE_ENABLE),
            (PadButton::Square, None, strings::HINT_ADD_BY_ID),
            (PadButton::R, None, strings::HINT_SCAN_QR),
            (PadButton::Triangle, None, strings::HINT_APPLY_SYNC),
            (PadButton::Circle, None, close_lbl),
        ],
    );
}
