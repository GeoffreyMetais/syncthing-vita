//! Modal Dialog: rear-camera viewfinder to scan a remote device's Syncthing Device ID QR code.

use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::image::draw_gray_image;
use crate::ui::strings;
use crate::ui::theme::{scan_qr_modal_rect, UiTheme, C_WARN, SCAN_QR_VIEW_H, SCAN_QR_VIEW_W};
use crate::ui::widgets::{action_width, draw_action, draw_hints};

/// Renders the `ScanDeviceQr` modal: live camera preview with a viewfinder frame.
pub fn render_scan_qr_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    gfx.dim(180);
    let (dx, dy, w, h) = scan_qr_modal_rect();
    let (dx, dy, w, h) = (dx as i32, dy as i32, w as i32, h as i32);

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 38) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Phone,
        (lx + 20) as f32,
        (dy + 38) as f32,
        20.0,
        th.accent_hi,
    );
    gfx.text(
        lx + 52,
        dy + 29,
        20,
        FontWeight::Bold,
        th.text,
        Align::Left,
        strings::MODAL_SCAN_QR_TITLE,
    );
    gfx.text_fit(
        lx + 52,
        dy + 51,
        w - 108,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        strings::MODAL_SCAN_QR_SUBTITLE,
    );

    let vx = lx;
    let vy = dy + 76;
    draw_viewfinder(gfx, th, state, vx, vy);

    let w_back = action_width(gfx, PadButton::Circle, strings::ACTION_BACK);
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_back,
        dy + h - 28,
        PadButton::Circle,
        strings::ACTION_BACK,
    );

    draw_hints(gfx, th, &[(PadButton::Circle, None, strings::HINT_BACK_TO_DEVICE_ID)]);
}

/// Camera frame (or status text) with corner brackets marking where to place the QR code.
fn draw_viewfinder(gfx: &mut Gfx, th: &UiTheme, state: &AppState, vx: i32, vy: i32) {
    let (vw, vh) = (SCAN_QR_VIEW_W, SCAN_QR_VIEW_H);
    gfx.rect(vx, vy, vw, vh, th.bg);

    if let Some(err) = state.qr_camera_error.as_deref() {
        let msg = format!("{} · Enter the ID manually instead", err);
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
            "Starting rear camera...",
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
