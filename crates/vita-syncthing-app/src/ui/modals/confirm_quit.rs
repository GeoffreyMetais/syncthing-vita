//! Modal Dialog: Confirmation before exiting VitaSync (`START` button).

use crate::domain::AppState;
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, CONFIRM_QUIT_H, CONFIRM_QUIT_W, C_BAD, C_WARN, HINT_Y};
use crate::ui::widgets::{action_width, draw_action, draw_hints};

/// Horizontal padding inside the dialog.
const PAD_X: i32 = 28;
/// Offset of the title / subtitle from the left padding (after the icon).
const HEADER_TEXT_DX: i32 = 52;

/// Modal Dialog 4: Confirmation before exiting VitaSync (`START` button).
pub fn render_confirm_quit_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let s = strings::strings(state.language);
    gfx.dim(180);
    let (body_title, body_hint) = if state.sync_in_progress {
        (s.modal_quit_syncing_warn, s.modal_quit_syncing_hint)
    } else {
        (s.modal_quit_idle_ready, s.modal_quit_idle_hint)
    };
    let w = CONFIRM_QUIT_W;
    let h = CONFIRM_QUIT_H;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + PAD_X;
    let header_max_w = w - 2 * PAD_X - HEADER_TEXT_DX;
    let body_max_w = w - 2 * PAD_X;
    gfx.circle(
        (lx + 20) as f32,
        (dy + 40) as f32,
        20.0,
        rgba(239, 83, 80, 42),
    );
    gfx.icon(Icon::Close, (lx + 20) as f32, (dy + 40) as f32, 22.0, C_BAD);
    gfx.text_fit(
        lx + HEADER_TEXT_DX,
        dy + 31,
        header_max_w,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        s.modal_quit_title,
    );
    gfx.text_fit(
        lx + HEADER_TEXT_DX,
        dy + 54,
        header_max_w,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_quit_subtitle,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    let body_colour = if state.sync_in_progress {
        C_WARN
    } else {
        th.text
    };
    gfx.text_fit(
        lx,
        dy + 111,
        body_max_w,
        15,
        FontWeight::Medium,
        body_colour,
        Align::Left,
        body_title,
    );
    gfx.text_fit(
        lx,
        dy + 137,
        body_max_w,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        body_hint,
    );

    let w_quit = action_width(gfx, PadButton::Cross, s.action_quit_app);
    let w_cancel = action_width(gfx, PadButton::Circle, s.action_cancel);
    draw_action(
        gfx,
        th,
        dx + w - PAD_X - w_quit - 12 - w_cancel,
        dy + h - 34,
        PadButton::Circle,
        s.action_cancel,
    );
    draw_action(
        gfx,
        th,
        dx + w - PAD_X - w_quit,
        dy + h - 34,
        PadButton::Cross,
        s.action_quit_app,
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Cross, None, s.hint_confirm_quit),
            (PadButton::Circle, None, s.action_cancel),
        ],
    );
}
