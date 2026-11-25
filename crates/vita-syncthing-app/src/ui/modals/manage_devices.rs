//! Modal Dialog: Add or remove Syncthing devices for a share (`shares[share_idx]`).

use crate::domain::interaction::{HitMap, PadKey, UiAction};
use crate::domain::{AppState, ShareMode, VISIBLE_DEVICES};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n;
use crate::ui::i18n::messages::fill;
use crate::ui::theme::{UiTheme, MANAGE_DEVICES_H, MANAGE_DEVICES_W};
use crate::ui::widgets::{
    action_width, declare_touch, draw_chip, draw_modal_card, draw_modal_header,
    draw_modal_separator, draw_press_button, draw_press_hints, draw_row_tick, ModalHeader,
    MODAL_HEADER_TEXT_DX, MODAL_PAD_X,
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
    let s = i18n::strings(state.language);
    let w = MANAGE_DEVICES_W;
    let h = MANAGE_DEVICES_H;
    let (dx, dy) = draw_modal_card(gfx, th, hits, w, h, 175, Some(PadKey::Circle), None);

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

    let lx = dx + MODAL_PAD_X;
    // Header right: the share's access mode; touching it (or `L`) cycles ReadWrite -> ReadOnly -> SendOnly.
    // A read-only share can also revert its local changes (`START`, confirmed first).
    let mode = state
        .shares
        .get(share_idx)
        .map_or(ShareMode::ReadWrite, |sh| sh.mode);
    let mode_lbl = match mode {
        ShareMode::ReadWrite => s.share_mode_read_write,
        ShareMode::ReadOnly => s.share_mode_read_only,
        ShareMode::SendOnly => s.share_mode_send_only,
    };
    let w_mode = action_width(gfx, PadButton::L, mode_lbl);
    let mut header_right = dx + w - 28 - w_mode;
    draw_press_button(gfx, th, hits, header_right, dy + 40, PadButton::L, mode_lbl);
    if mode == ShareMode::ReadOnly {
        let w_revert = action_width(gfx, PadButton::Start, s.action_revert);
        header_right -= 10 + w_revert;
        draw_press_button(
            gfx,
            th,
            hits,
            header_right,
            dy + 40,
            PadButton::Start,
            s.action_revert,
        );
    }
    let header_w = header_right - 12 - (lx + MODAL_HEADER_TEXT_DX);

    let header_title = s.fmt_devices_for_share(&share_title);
    let incomplete = state.is_share_creation_incomplete(share_idx);
    let (subtitle, subtitle_col) = if incomplete {
        (s.modal_manage_devices_subtitle_incomplete, th.warn)
    } else {
        (s.modal_manage_devices_subtitle, th.dim)
    };
    draw_modal_header(
        gfx,
        th,
        dx,
        dy,
        header_w,
        &ModalHeader::new(
            Icon::Desktop,
            22.0,
            th.accent_hi,
            th.accent_bg,
            &header_title,
        )
        .subtitle(subtitle, subtitle_col),
    );
    draw_modal_separator(gfx, th, dx, dy, w, 76);

    if state.devices.is_empty() {
        let (headline, col) = if state.sync_in_progress {
            (s.modal_manage_devices_scanning, th.accent_hi)
        } else {
            (s.modal_manage_devices_empty, th.warn)
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
            let is_checked = shared_list.iter().any(|id| id.matches_peer(&dev.device_id));

            if is_sel {
                gfx.rrect(lx, ry, w - 56, 46, th.card_radius.min(10), th.accent_bg);
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
            let badge_col = if is_checked { th.good } else { th.dim };
            draw_chip(gfx, th, dx + w - 36, cy, badge_txt, badge_col);

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
    let close_lbl = if incomplete {
        s.action_cancel
    } else {
        s.action_done
    };
    let w_close = action_width(gfx, PadButton::Circle, close_lbl);
    let bx_close = dx + w - 28 - w_close;
    let bx_add = bx_close - 10 - w_add;
    let w_scan = action_width(gfx, PadButton::R, s.action_scan_qr);
    let bx_scan = bx_add - 10 - w_scan;
    // With very long labels the row has no room for `Scan QR`: it stays available with `R`
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
        bx_close,
        dy + h - 32,
        PadButton::Circle,
        close_lbl,
    );

    // No device to tick yet: `Triangle` scans the LAN (while no sync is running).
    let empty_hints = [
        (PadButton::Triangle, None, s.hint_scan_lan),
        (PadButton::R, None, s.hint_scan_qr),
        (PadButton::Circle, None, close_lbl),
    ];
    let device_hints = [
        (PadButton::DpadUd, None, s.hint_select),
        (PadButton::Cross, None, s.hint_toggle),
        (PadButton::R, None, s.hint_scan_qr),
        (PadButton::Circle, None, close_lbl),
    ];
    let hints: &[_] = match (state.devices.is_empty(), state.sync_in_progress) {
        (true, false) => &empty_hints,
        (true, true) => &empty_hints[1..],
        (false, _) => &device_hints,
    };
    draw_press_hints(gfx, th, hits, hints);
}
