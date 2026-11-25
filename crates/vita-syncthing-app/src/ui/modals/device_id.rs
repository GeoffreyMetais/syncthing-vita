//! Modal Dialog: Display the PS Vita's Syncthing Device ID + scannable QR code.

use qrcodegen::{QrCode, QrCodeEcc};

use crate::domain::interaction::{HitMap, PadKey};
use crate::domain::AppState;
use crate::ui::gfx::{rgb, Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{
    action_width, draw_chip, draw_modal_card, draw_modal_header, draw_modal_separator,
    draw_press_button, draw_press_hints, ModalHeader,
};

/// Modal Dialog 3: Display the PS Vita's Syncthing Device ID + scannable QR code.
pub fn render_device_id_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = i18n::strings(state.language);
    let w = 720;
    let h = 446;
    let (dx, dy) = draw_modal_card(
        gfx,
        th,
        hits,
        w,
        h,
        180,
        Some(PadKey::Circle),
        Some(PadKey::Circle),
    );

    let lx = draw_modal_header(
        gfx,
        th,
        dx,
        dy,
        w - 108,
        &ModalHeader::new(
            Icon::Vita,
            26.0,
            th.accent_hi,
            th.accent_bg,
            s.modal_device_id_title,
        )
        .subtitle(s.modal_device_id_subtitle, th.dim),
    );
    draw_modal_separator(gfx, th, dx, dy, w, 76);

    // Left Column: Crisp white QR code card (`246 x 246`)
    let qr_box_x = lx;
    let qr_box_y = dy + 94;
    let qr_box_size = 246;
    gfx.rrect(
        qr_box_x,
        qr_box_y,
        qr_box_size,
        qr_box_size,
        th.card_radius,
        rgb(255, 255, 255),
    );
    gfx.rrect_outline(
        qr_box_x,
        qr_box_y,
        qr_box_size,
        qr_box_size,
        th.card_radius,
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

    let inner_r = th.card_radius.min(10);

    // Row 1 box (Groups 1..4)
    let box1_y = dy + 120;
    gfx.rrect(rx, box1_y, rw, 50, inner_r, th.accent_bg);
    gfx.rrect_outline(rx, box1_y, rw, 50, inner_r, 1, th.accent);
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
    gfx.rrect(rx, box2_y, rw, 50, inner_r, th.accent_bg);
    gfx.rrect_outline(rx, box2_y, rw, 50, inner_r, 1, th.accent);
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
    gfx.rrect(rx, meta_y, rw, 96, inner_r, th.bg);
    gfx.rrect_outline(rx, meta_y, rw, 96, inner_r, 1, th.border);

    let short_id = groups.first().copied().unwrap_or("VITA");
    draw_chip(
        gfx,
        th,
        rx + rw - 14,
        meta_y + 22,
        "TLS 1.3 · ECDSA P-256",
        th.good,
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

    // Bottom buttons at bottom-right of modal: [R Scan QR] [□ Add remote device] [○ Close]
    let w_add = action_width(gfx, PadButton::Square, s.action_add_remote_device);
    let w_scan = action_width(gfx, PadButton::R, s.action_scan_qr);
    let w_close = action_width(gfx, PadButton::Circle, s.action_close);
    let close_x = dx + w - 28 - w_close;
    let add_x = close_x - 10 - w_add;
    let scan_x = add_x - 10 - w_scan;
    let buttons_y = dy + h - 32;
    draw_press_button(
        gfx,
        th,
        hits,
        add_x,
        buttons_y,
        PadButton::Square,
        s.action_add_remote_device,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        scan_x,
        buttons_y,
        PadButton::R,
        s.action_scan_qr,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        close_x,
        buttons_y,
        PadButton::Circle,
        s.action_close,
    );

    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::R, None, s.hint_scan_qr),
            (PadButton::Square, None, s.hint_add_remote_device_id),
            (
                PadButton::Circle,
                Some(PadButton::Cross),
                s.hint_close_qr_device_id,
            ),
        ],
    );
}
