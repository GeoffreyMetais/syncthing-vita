//! Modal Dialog: rear-camera viewfinder to scan a remote device's Syncthing Device ID QR code.

use crate::domain::interaction::{HitMap, PadKey};
use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n;
use crate::ui::i18n::messages::fill;
use crate::ui::image::draw_gray_image;
use crate::ui::theme::{scan_qr_modal_rect, UiTheme, C_WARN, SCAN_QR_VIEW_H, SCAN_QR_VIEW_W};
use crate::ui::widgets::{
    action_width, draw_modal_card, draw_modal_header, draw_press_button, draw_press_hints,
    ModalHeader,
};

/// Renders the `ScanDeviceQr` modal: live camera preview with a viewfinder frame.
pub fn render_scan_qr_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = i18n::strings(state.language);
    let (_, _, w, h) = scan_qr_modal_rect();
    let (w, h) = (w as i32, h as i32);
    let (dx, dy) = draw_modal_card(gfx, th, hits, w, h, 180, Some(PadKey::Circle), None);

    let lx = draw_modal_header(
        gfx,
        th,
        dx,
        dy,
        w - 108,
        &ModalHeader::new(
            Icon::Phone,
            20.0,
            th.accent_hi,
            th.accent_bg,
            s.modal_scan_qr_title,
        )
        .compact()
        .subtitle(s.modal_scan_qr_subtitle, th.dim),
    );

    let vx = lx;
    let vy = dy + 76;
    draw_viewfinder(gfx, th, state, vx, vy);

    let w_back = action_width(gfx, PadButton::Circle, s.action_back);
    draw_press_button(
        gfx,
        th,
        hits,
        dx + w - 28 - w_back,
        dy + h - 28,
        PadButton::Circle,
        s.action_back,
    );

    draw_press_hints(
        gfx,
        th,
        hits,
        &[(PadButton::Circle, None, s.hint_back_to_device_id)],
    );
}

/// Camera frame (or status text) with corner brackets marking where to place the QR code.
fn draw_viewfinder(gfx: &mut Gfx, th: &UiTheme, state: &AppState, vx: i32, vy: i32) {
    let (vw, vh) = (SCAN_QR_VIEW_W, SCAN_QR_VIEW_H);
    gfx.rect(vx, vy, vw, vh, th.bg);

    let st = i18n::strings(state.language);
    if let Some(err) = state.qr_camera_error.as_deref() {
        let msg = fill(st.messages.camera_error, &[("error", &err)]);
        gfx.text_fit(
            vx + vw / 2,
            vy + vh / 2,
            vw - 32,
            15,
            FontWeight::Medium,
            C_WARN,
            Align::Center,
            &msg,
        );
        return;
    }
    if !state.qr_preview.has_frame() {
        gfx.text(
            vx + vw / 2,
            vy + vh / 2,
            15,
            FontWeight::Medium,
            th.dim,
            Align::Center,
            st.messages.camera_starting,
        );
        return;
    }

    draw_gray_image(gfx, vx, vy, vw, vh, &state.qr_preview);

    // Square target in the centre with accent corner brackets
    let side = vh - 60;
    let tx = (vx + (vw - side) / 2) as f32;
    let ty = (vy + 30) as f32;
    let s = side as f32;
    let arm = 28.0;
    let c = th.accent;
    for (cx, cy, sx, sy) in [
        (tx, ty, 1.0, 1.0),
        (tx + s, ty, -1.0, 1.0),
        (tx, ty + s, 1.0, -1.0),
        (tx + s, ty + s, -1.0, -1.0),
    ] {
        gfx.line(cx, cy, cx + arm * sx, cy, 4.0, c);
        gfx.line(cx, cy, cx, cy + arm * sy, 4.0, c);
    }
}
