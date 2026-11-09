//! Modal Dialog: Confirmation before exiting VitaSync (`START` button).

use crate::domain::AppState;
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
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
        "Quit VitaSync?",
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "Confirm whether to close the application and return to LiveArea",
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
            "A BEP/1.0 synchronization is currently in progress.",
        );
        gfx.text(
            lx,
            dy + 136,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            "Quitting now will interrupt active file transfers.",
        );
    } else {
        gfx.text(
            lx,
            dy + 112,
            15,
            FontWeight::Medium,
            th.text,
            Align::Left,
            "Are you sure you want to exit VitaSync?",
        );
        gfx.text(
            lx,
            dy + 138,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            "All share configurations and device settings are saved on ux0:.",
        );
    }

    let w_quit = action_width(gfx, PadButton::Cross, "Quit App");
    let w_cancel = action_width(gfx, PadButton::Circle, "Cancel");
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_quit - 12 - w_cancel,
        dy + h - 34,
        PadButton::Circle,
        "Cancel",
    );
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_quit,
        dy + h - 34,
        PadButton::Cross,
        "Quit App",
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Cross, None, "Confirm Quit"),
            (PadButton::Circle, None, "Cancel"),
        ],
    );
}
