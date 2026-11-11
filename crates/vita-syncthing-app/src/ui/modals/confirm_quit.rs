//! Modal Dialog: Confirmation before exiting VitaSync (`START` button).

use crate::domain::AppState;
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_BAD, C_WARN, HINT_Y};
use crate::ui::widgets::{action_width, draw_action, draw_hints};

/// Modal Dialog 4: Confirmation before exiting VitaSync (`START` button).
pub fn render_confirm_quit_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    gfx.dim(180);
    let w = 520;
    let h = 236;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle(
        (lx + 20) as f32,
        (dy + 40) as f32,
        20.0,
        rgba(239, 83, 80, 42),
    );
    gfx.icon(Icon::Close, (lx + 20) as f32, (dy + 40) as f32, 22.0, C_BAD);
    gfx.text(
        lx + 52,
        dy + 31,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        strings::MODAL_QUIT_TITLE,
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        strings::MODAL_QUIT_SUBTITLE,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    if state.sync_in_progress {
        gfx.text(
            lx,
            dy + 110,
            15,
            FontWeight::Medium,
            C_WARN,
            Align::Left,
            strings::MODAL_QUIT_SYNCING_WARN,
        );
        gfx.text(
            lx,
            dy + 136,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            strings::MODAL_QUIT_SYNCING_HINT,
        );
    } else {
        gfx.text(
            lx,
            dy + 112,
            15,
            FontWeight::Medium,
            th.text,
            Align::Left,
            strings::MODAL_QUIT_IDLE_READY,
        );
        gfx.text(
            lx,
            dy + 138,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            strings::MODAL_QUIT_IDLE_HINT,
        );
    }

    let w_quit = action_width(gfx, PadButton::Cross, strings::ACTION_QUIT_APP);
    let w_cancel = action_width(gfx, PadButton::Circle, strings::ACTION_CANCEL);
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_quit - 12 - w_cancel,
        dy + h - 34,
        PadButton::Circle,
        strings::ACTION_CANCEL,
    );
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_quit,
        dy + h - 34,
        PadButton::Cross,
        strings::ACTION_QUIT_APP,
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Cross, None, strings::HINT_CONFIRM_QUIT),
            (PadButton::Circle, None, strings::ACTION_CANCEL),
        ],
    );
}
