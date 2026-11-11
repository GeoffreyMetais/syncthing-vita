//! Modal Dialog: Add a remote Syncthing device by entering its Short ID (7+ chars) or Full ID (56 chars).

use syncthing_core::DeviceId;

use super::fallback_keyboard::draw_fallback_keypad;
use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{add_device_modal_height, UiTheme, C_GOOD, C_WARN, HINT_Y};
use crate::ui::widgets::{action_width, draw_action, draw_hints};

/// Modal Dialog 2b: Add a remote Syncthing device by entering its Short ID (7+ chars) or Full ID (56 chars).
pub fn render_add_device_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState, keypad_idx: usize) {
    gfx.dim(180);
    let keypad = state.fallback_keypad_visible;
    let w = 700;
    let h = add_device_modal_height(keypad);
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    let content_w = w - 56;
    gfx.circle((lx + 20) as f32, (dy + 38) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Plus,
        (lx + 20) as f32,
        (dy + 38) as f32,
        22.0,
        th.accent_hi,
    );
    gfx.text(
        lx + 52,
        dy + 29,
        20,
        FontWeight::Bold,
        th.text,
        Align::Left,
        strings::MODAL_ADD_DEVICE_TITLE,
    );
    gfx.text(
        lx + 52,
        dy + 51,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        strings::MODAL_ADD_DEVICE_SUBTITLE,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 70) as f32,
        (dx + w - 24) as f32,
        (dy + 70) as f32,
        1.0,
        th.border,
    );

    // Live Text Input Box (y = dy + 80..128)
    let box_y = dy + 80;
    let box_h = 48;
    let id_part = state.device_id_input.split('@').next().unwrap_or("").trim();
    let parse_res = DeviceId::parse_user_id(id_part);
    let border_col = if state.device_id_input.trim().is_empty() {
        th.accent
    } else if parse_res.is_ok() {
        C_GOOD
    } else {
        C_WARN
    };

    gfx.rrect(lx, box_y, content_w, box_h, 10, th.bg);
    gfx.rrect_outline(lx, box_y, content_w, box_h, 10, 1, border_col);

    let kbd_lbl = if keypad {
        strings::ACTION_HIDE_KEYBOARD
    } else {
        strings::ACTION_VIRTUAL_KEYBOARD
    };
    let w_kbd = action_width(gfx, PadButton::Triangle, kbd_lbl);
    draw_action(
        gfx,
        th,
        lx + content_w - 10 - w_kbd,
        box_y + box_h / 2,
        PadButton::Triangle,
        kbd_lbl,
    );

    let text_max_w = content_w - w_kbd - 32;
    if state.device_id_input.is_empty() {
        gfx.text_fit(
            lx + 14,
            box_y + box_h / 2,
            text_max_w,
            15,
            FontWeight::Regular,
            th.faint,
            Align::Left,
            strings::MODAL_ADD_DEVICE_EXAMPLE,
        );
    } else {
        let display_str = format!("{}|", state.device_id_input.to_uppercase());
        gfx.text_fit(
            lx + 14,
            box_y + box_h / 2,
            text_max_w,
            17,
            FontWeight::Bold,
            th.text,
            Align::Left,
            &display_str,
        );
    }

    // Live Validation & LAN Peer Resolution Status Line
    let (status_txt, status_col) = if state.device_id_input.trim().is_empty() {
        (
            strings::MODAL_ADD_DEVICE_TIP.to_owned(),
            th.dim,
        )
    } else {
        match parse_res {
            Ok(parsed_id) => {
                if let Some(peer) = state.devices.iter().find(|d| {
                    d.device_id.matches_peer(&parsed_id) && !d.device_id.is_short_placeholder()
                }) {
                    (
                        format!(
                            "Matches discovered LAN peer: {} ({} @ {}) · Ready to link!",
                            peer.name,
                            peer.device_id.short(),
                            peer.address
                        ),
                        C_GOOD,
                    )
                } else if parsed_id.is_short_placeholder() {
                    (
                        format!(
                            "Valid Short ID ({} chars: {}) · Will auto-pair with matching peer on LAN",
                            parsed_id.syncthing_prefix().len(),
                            parsed_id.short()
                        ),
                        C_GOOD,
                    )
                } else {
                    (
                        format!(
                            "Valid Full 56-character Syncthing Device ID ({})",
                            parsed_id.short()
                        ),
                        C_GOOD,
                    )
                }
            }
            Err(err) => (err, C_WARN),
        }
    };
    gfx.text_fit(
        lx + 4,
        dy + 143,
        content_w - 8,
        13,
        FontWeight::Medium,
        status_col,
        Align::Left,
        &status_txt,
    );

    // Render the dedicated fallback Base32 keypad component (`Virtual Keyboard`) on demand
    if keypad {
        draw_fallback_keypad(gfx, th, lx, dy + 162, keypad_idx, state.ime_active);
    }

    // Bottom modal action buttons
    let w_confirm = action_width(gfx, PadButton::Start, strings::MODAL_ADD_DEVICE_CONFIRM);
    let w_bksp = action_width(gfx, PadButton::Square, strings::ACTION_BACKSPACE);
    let w_cancel = action_width(gfx, PadButton::Circle, strings::ACTION_CANCEL);
    let bx_cancel = dx + w - 28 - w_cancel;
    let bx_bksp = bx_cancel - 10 - w_bksp;
    let bx_confirm = bx_bksp - 10 - w_confirm;

    draw_action(
        gfx,
        th,
        bx_confirm,
        dy + h - 28,
        PadButton::Start,
        strings::MODAL_ADD_DEVICE_CONFIRM,
    );
    draw_action(
        gfx,
        th,
        bx_bksp,
        dy + h - 28,
        PadButton::Square,
        strings::ACTION_BACKSPACE,
    );
    draw_action(gfx, th, bx_cancel, dy + h - 28, PadButton::Circle, strings::ACTION_CANCEL);
    draw_action(gfx, th, lx, dy + h - 28, PadButton::R, strings::ACTION_SCAN_QR);

    let mut hints = vec![
        (PadButton::Triangle, None, kbd_lbl),
        (PadButton::Start, None, strings::HINT_CONFIRM_ADD),
        (PadButton::R, None, strings::ACTION_SCAN_QR),
    ];
    if keypad {
        hints.push((PadButton::Dpad, Some(PadButton::Cross), strings::HINT_TYPE_KEY));
    }
    hints.push((PadButton::Square, None, strings::ACTION_BACKSPACE));
    hints.push((PadButton::Circle, None, strings::ACTION_CANCEL));
    draw_hints(gfx, th, &hints);
}
