//! Modal Dialog: Confirmation before deleting a share (`Square` on the `Overview` share list).

use super::confirm_frame::{draw_confirm_frame, draw_confirm_lines, ConfirmFrame, CONFIRM_PAD_X};
use crate::domain::interaction::HitMap;
use crate::domain::AppState;
use crate::ui::gfx::{with_alpha, FontWeight, Gfx, Icon};
use crate::ui::i18n;
use crate::ui::theme::UiTheme;

/// Renders the `ConfirmDeleteShare` modal for `shares[share_idx]`.
pub fn render_confirm_delete_share_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
    hits: &mut HitMap,
) {
    let s = i18n::strings(state.language);
    // Wide enough for the longest subtitle (Spanish) next to the icon.
    let w = 620;
    let frame = ConfirmFrame {
        size: (w, 236),
        icon: Icon::Close,
        icon_colour: th.bad,
        icon_disc: with_alpha(th.bad, 42),
        title: s.modal_delete_share_title,
        subtitle: s.modal_delete_share_subtitle,
        confirm_label: s.action_delete,
        confirm_hint: s.hint_confirm_delete,
    };
    let (dx, dy) = draw_confirm_frame(gfx, th, hits, state.language, &frame);

    if let Some(share) = state.shares.get(share_idx) {
        let title = s.fmt_share_title(&share.label, &share.folder_id);
        let files_msg = s.fmt_files_kept_on_vita(&share.target_path);
        let decline_hint = s.modal_delete_share_decline_hint;
        draw_confirm_lines(
            gfx,
            dx + CONFIRM_PAD_X,
            w - 2 * CONFIRM_PAD_X,
            &[
                (dy + 110, 16, FontWeight::Medium, th.text, &title),
                (dy + 136, 14, FontWeight::Regular, th.dim, &files_msg),
                (dy + 158, 13, FontWeight::Regular, th.faint, decline_hint),
            ],
        );
    }
}
