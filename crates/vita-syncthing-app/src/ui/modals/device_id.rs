//! Modal Dialog: Display the PS Vita's Syncthing Device ID + scannable QR code.

use qrcodegen::{QrCode, QrCodeEcc};

use crate::domain::AppState;
use crate::ui::gfx::{rgb, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_GOOD, HINT_Y};
use crate::ui::widgets::{action_width, draw_action, draw_chip, draw_hints};

/// Modal Dialog 3: Display the PS Vita's Syncthing Device ID + scannable QR code.
pub fn render_device_id_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let s = strings::strings(state.language);
    gfx.dim(180);
    let w = 720;
    let h = 414;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Vita,
        (lx + 20) as f32,
        (dy + 40) as f32,
        26.0,
        th.accent_hi,
    );
    gfx.text(
        lx + 52,
        dy + 31,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        s.modal_device_id_title,
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_device_id_subtitle,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    // Left Column: Crisp white QR code card (`246 x 246`)
    let qr_box_x = lx;
    let qr_box_y = dy + 94;
    let qr_box_size = 246;
    gfx.rrect(
        qr_box_x,
        qr_box_y,
        qr_box_size,
        qr_box_size,
        14,
        rgb(255, 255, 255),
    );
    gfx.rrect_outline(
        qr_box_x,
        qr_box_y,
        qr_box_size,
        qr_box_size,
        14,
        1,
        th.border,
    );

    let dev_id = &state.vita_device_id;
    if let Ok(qr) = QrCode::encode_text(dev_id, QrCodeEcc::Medium) {
        let n = qr.size();
        let scale = ((qr_box_size - 26) / n).max(3);
        let total_px = n * scale;
        let ox = qr_box_x + (qr_box_size - total_px) / 2;
        let oy = qr_box_y + (qr_box_size - total_px) / 2;
        let dark = rgb(10, 14, 18);

        for my in 0..n {
            for mx in 0..n {
                if qr.get_module(mx, my) {
                    gfx.rect(ox + mx * scale, oy + my * scale, scale, scale, dark);
                }
            }
        }
    }

    gfx.text_fit(
        qr_box_x + qr_box_size / 2,
        qr_box_y + qr_box_size + 22,
        qr_box_size + 20,
        13,
        FontWeight::Medium,
        th.accent_hi,
        Align::Center,
        s.modal_device_id_scan_web,
    );

    // Right Column: Formatted 8-group Syncthing Device ID + Certificate details
    let rx = qr_box_x + qr_box_size + 24;
    let rw = dx + w - 28 - rx;

    gfx.text(
        rx,
        dy + 104,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.messages.device_id_section,
    );

    let groups: Vec<&str> = dev_id.split('-').collect();
    let row1 = if groups.len() >= 4 {
        format!(
            "{}  -  {}  -  {}  -  {}",
            groups[0], groups[1], groups[2], groups[3]
        )
    } else {
        dev_id.clone()
    };
    let row2 = if groups.len() >= 8 {
        format!(
            "{}  -  {}  -  {}  -  {}",
            groups[4], groups[5], groups[6], groups[7]
        )
    } else {
        String::new()
    };

    // Row 1 box (Groups 1..4)
    let box1_y = dy + 120;
    gfx.rrect(rx, box1_y, rw, 50, 10, th.accent_bg);
    gfx.rrect_outline(rx, box1_y, rw, 50, 10, 1, th.accent);
    gfx.text(
        rx + 14,
        box1_y + 15,
        11,
        FontWeight::Medium,
        th.dim,
        Align::Left,
        s.messages.device_id_groups_1_4,
    );
    gfx.text_fit(
        rx + 14,
        box1_y + 35,
        rw - 28,
        16,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &row1,
    );

    // Row 2 box (Groups 5..8)
    let box2_y = dy + 180;
    gfx.rrect(rx, box2_y, rw, 50, 10, th.accent_bg);
    gfx.rrect_outline(rx, box2_y, rw, 50, 10, 1, th.accent);
    gfx.text(
        rx + 14,
        box2_y + 15,
        11,
        FontWeight::Medium,
        th.dim,
        Align::Left,
        s.messages.device_id_groups_5_8,
    );
    gfx.text_fit(
        rx + 14,
        box2_y + 35,
        rw - 28,
        16,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &row2,
    );

    // Device metadata card
    let meta_y = dy + 244;
    gfx.rrect(rx, meta_y, rw, 96, 10, th.bg);
    gfx.rrect_outline(rx, meta_y, rw, 96, 10, 1, th.border);

    let short_id = groups.first().copied().unwrap_or("VITA");
    draw_chip(
        gfx,
        rx + rw - 14,
        meta_y + 22,
        "TLS 1.3 · ECDSA P-256",
        C_GOOD,
    );
    let short_id_label = s.fmt_short_id_label(short_id);
    gfx.text(
        rx + 14,
        meta_y + 22,
        14,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &short_id_label,
    );
    gfx.text(
        rx + 14,
        meta_y + 48,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_device_id_dev_name,
    );
    gfx.text(
        rx + 14,
        meta_y + 72,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        s.modal_device_id_cert_path,
    );

    // Bottom buttons at bottom-right of modal
    let w_add = action_width(gfx, PadButton::Square, s.action_add_remote_device);
    let w_close = action_width(gfx, PadButton::Circle, s.action_close);
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_close - 10 - w_add,
        dy + h - 32,
        PadButton::Square,
        s.action_add_remote_device,
    );
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_close,
        dy + h - 32,
        PadButton::Circle,
        s.action_close,
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Square, None, s.hint_add_remote_device_id),
            (
                PadButton::Circle,
                Some(PadButton::Cross),
                s.hint_close_qr_device_id,
            ),
        ],
    );
}
