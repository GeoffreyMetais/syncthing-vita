//! Modal Dialog: the console has no Wi-Fi connection (`WifiOff`), with the steps to turn it on.
//! It closes by itself once connected (`application::WifiPrompt`).

use crate::domain::interaction::HitMap;
use crate::domain::AppState;
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_WARN, HINT_Y, WIFI_OFF_H, WIFI_OFF_W};
use crate::ui::widgets::{action_width, declare_modal, draw_press_button, draw_press_hints};

/// Horizontal padding inside the dialog.
const PAD_X: i32 = 28;
/// Offset of the title / subtitle from the left padding (after the icon).
const HEADER_TEXT_DX: i32 = 52;
/// Offset of the instruction lines from the left padding (after the step number).
const STEP_TEXT_DX: i32 = 30;

/// Renders the `WifiOff` modal.
pub fn render_wifi_off_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    gfx.dim(180);
    let (w, h) = (WIFI_OFF_W, WIFI_OFF_H);
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    // A stray touch must not hide the instructions: only the buttons answer.
    declare_modal(hits, (dx, dy, w, h), None, None);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + PAD_X;
    let header_max_w = w - 2 * PAD_X - HEADER_TEXT_DX;
    let body_max_w = w - 2 * PAD_X;
    gfx.circle(
        (lx + 20) as f32,
        (dy + 40) as f32,
        20.0,
        rgba(255, 193, 77, 42),
    );
    gfx.icon(Icon::Wifi, (lx + 20) as f32, (dy + 40) as f32, 22.0, C_WARN);
    gfx.text_fit(
        lx + HEADER_TEXT_DX,
        dy + 31,
        header_max_w,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        s.modal_wifi_title,
    );
    gfx.text_fit(
        lx + HEADER_TEXT_DX,
        dy + 54,
        header_max_w,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_wifi_subtitle,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    let steps = [s.modal_wifi_step_open, s.modal_wifi_step_enable];
    for (i, step) in steps.iter().enumerate() {
        let y = dy + 108 + 30 * i as i32;
        gfx.text(
            lx,
            y,
            15,
            FontWeight::Bold,
            th.accent,
            Align::Left,
            &format!("{}.", i + 1),
        );
        gfx.text_fit(
            lx + STEP_TEXT_DX,
            y,
            body_max_w - STEP_TEXT_DX,
            15,
            FontWeight::Medium,
            th.text,
            Align::Left,
            step,
        );
    }
    gfx.text_fit(
        lx,
        dy + 176,
        body_max_w,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_wifi_waiting,
    );

    let w_offline = action_width(gfx, PadButton::Circle, s.action_continue_offline);
    draw_press_button(
        gfx,
        th,
        hits,
        dx + w - PAD_X - w_offline,
        dy + h - 34,
        PadButton::Circle,
        s.action_continue_offline,
    );

    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::Circle, None, s.action_continue_offline),
            (PadButton::Start, None, s.action_quit),
        ],
    );
}
