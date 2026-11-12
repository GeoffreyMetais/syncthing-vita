//! Modal Dialog: Add or remove Syncthing devices for a share (`shares[share_idx]`).

use syncthing_core::DeviceId;

use crate::domain::interaction::{HitMap, PadKey, UiAction};
use crate::domain::{AppState, VISIBLE_DEVICES};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::i18n::messages::fill;
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_GOOD, C_WARN, HINT_Y, MANAGE_DEVICES_H, MANAGE_DEVICES_W};
use crate::ui::widgets::{
    action_width, declare_modal, declare_touch, draw_chip, draw_press_button, draw_press_hints,
    draw_row_tick,
};

/// Height of the touch target of a device row.
const DEVICE_ROW_H: i32 = 46;

/// Modal Dialog 2: Add or remove Syncthing devices for a share (`shares[share_idx]`).
pub fn render_manage_devices_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
    selected_dev_idx: usize,
    hits: &mut HitMap,
) {
    let s = strings::strings(state.language);
    gfx.dim(175);
    let w = MANAGE_DEVICES_W;
    let h = MANAGE_DEVICES_H;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    declare_modal(hits, (dx, dy, w, h), Some(PadKey::Circle), None);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let share_title = state
        .shares
        .get(share_idx)
        .map(|share| format!("{} ({})", share.label, share.folder_id))
        .unwrap_or_else(|| s.modal_manage_devices_default_title.to_owned());
    let shared_list = state
        .shares
        .get(share_idx)
        .map(|share| share.shared_devices.as_slice())
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
    let header_title = s.fmt_devices_for_share(&share_title);
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
        (s.modal_manage_devices_subtitle_incomplete, C_WARN)
    } else {
        (s.modal_manage_devices_subtitle, th.dim)
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
            (s.modal_manage_devices_scanning, th.accent_hi)
        } else {
            (s.modal_manage_devices_empty, C_WARN)
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
            s.text(&state.status_line)
        } else {
            s.modal_manage_devices_empty_hint.into()
        };
        gfx.text(
            lx,
            dy + 160,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            &sub_hint,
        );
    } else {
        for (i, dev) in state.devices.iter().take(VISIBLE_DEVICES).enumerate() {
            let ry = dy + 92 + (i as i32) * 52;
            let cy = ry + 23;
            // Touching a device links / unlinks it.
            declare_touch(hits, dx, ry, w, DEVICE_ROW_H, UiAction::ModalRow(i));
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

            let badge_txt = if is_checked {
                s.action_linked
            } else {
                s.action_not_linked
            };
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
                fill(
                    s.messages.device_row_short,
                    &[
                        ("prefix", &dev.device_id.syncthing_prefix()),
                        ("addr", &dev.address),
                    ],
                )
            } else {
                fill(
                    s.messages.device_row_full,
                    &[
                        ("id", &dev.device_id.to_syncthing_string()),
                        ("addr", &dev.address),
                    ],
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
    let pause_lbl = if state.shares.get(share_idx).is_some_and(|sh| sh.enabled) {
        s.action_pause
    } else {
        s.action_enable
    };
    let w_pause = draw_press_button(gfx, th, hits, lx, dy + h - 32, PadButton::Select, pause_lbl);
    let w_add = action_width(gfx, PadButton::Square, s.action_add_by_id);
    let w_sync = action_width(gfx, PadButton::Triangle, s.action_apply_sync);
    let close_lbl = if incomplete {
        s.action_cancel
    } else {
        s.action_done
    };
    let w_close = action_width(gfx, PadButton::Circle, close_lbl);
    let bx_close = dx + w - 28 - w_close;
    let bx_sync = bx_close - 10 - w_sync;
    let bx_add = bx_sync - 10 - w_add;
    let w_scan = action_width(gfx, PadButton::R, s.action_scan_qr);
    let bx_scan = bx_add - 10 - w_scan;
    // With long labels the row has no room for `Scan QR`: it stays available with `R`
    // and in the hint bar instead of covering the `Pause` button.
    if bx_scan >= lx + w_pause + 10 {
        draw_press_button(
            gfx,
            th,
            hits,
            bx_scan,
            dy + h - 32,
            PadButton::R,
            s.action_scan_qr,
        );
    }
    draw_press_button(
        gfx,
        th,
        hits,
        bx_add,
        dy + h - 32,
        PadButton::Square,
        s.action_add_by_id,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        bx_sync,
        dy + h - 32,
        PadButton::Triangle,
        s.action_apply_sync,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        bx_close,
        dy + h - 32,
        PadButton::Circle,
        close_lbl,
    );

    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::DpadUd, None, s.hint_select),
            (PadButton::Cross, None, s.hint_toggle),
            (PadButton::R, None, s.hint_scan_qr),
            (PadButton::Triangle, None, s.hint_apply_sync),
            (PadButton::Circle, None, close_lbl),
        ],
    );
}
