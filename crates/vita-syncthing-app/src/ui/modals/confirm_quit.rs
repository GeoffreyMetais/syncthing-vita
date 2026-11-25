//! Modal Dialog: Confirmation before exiting VitaSync (`START` button).

use super::confirm_frame::{draw_confirm_frame, draw_confirm_lines, ConfirmFrame, CONFIRM_PAD_X};
use crate::domain::interaction::HitMap;
use crate::domain::AppState;
use crate::ui::gfx::{rgba, FontWeight, Gfx, Icon};
use crate::ui::i18n;
use crate::ui::theme::{UiTheme, CONFIRM_QUIT_H, CONFIRM_QUIT_W, C_BAD, C_WARN};

/// Renders the `ConfirmQuit` modal (warns when a sync is in progress).
pub fn render_confirm_quit_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = i18n::strings(state.language);
    let w = CONFIRM_QUIT_W;
    let frame = ConfirmFrame {
        size: (w, CONFIRM_QUIT_H),
        icon: Icon::Close,
        icon_colour: C_BAD,
        icon_disc: rgba(239, 83, 80, 42),
        title: s.modal_quit_title,
        subtitle: s.modal_quit_subtitle,
        confirm_label: s.action_quit_app,
        confirm_hint: s.hint_confirm_quit,
    };
    let (dx, dy) = draw_confirm_frame(gfx, th, hits, state.language, &frame);

    let (body_title, body_hint, body_colour) = if state.sync_in_progress {
        (s.modal_quit_syncing_warn, s.modal_quit_syncing_hint, C_WARN)
    } else {
        (s.modal_quit_idle_ready, s.modal_quit_idle_hint, th.text)
    };
    draw_confirm_lines(
        gfx,
        dx + CONFIRM_PAD_X,
        w - 2 * CONFIRM_PAD_X,
        &[
            (dy + 111, 15, FontWeight::Medium, body_colour, body_title),
            (dy + 137, 14, FontWeight::Regular, th.dim, body_hint),
        ],
    );
}
