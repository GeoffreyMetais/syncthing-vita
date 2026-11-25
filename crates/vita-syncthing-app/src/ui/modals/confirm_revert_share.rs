//! Modal Dialog: Confirmation before reverting a read-only share's local changes
//! (`START` in `ManageDevices`).

use super::confirm_frame::{draw_confirm_frame, draw_confirm_lines, ConfirmFrame, CONFIRM_PAD_X};
use crate::domain::interaction::HitMap;
use crate::domain::AppState;
use crate::ui::gfx::{rgba, FontWeight, Gfx, Icon};
use crate::ui::i18n;
use crate::ui::theme::{UiTheme, C_WARN};

/// Renders the `ConfirmRevertShare` modal for `shares[share_idx]`.
pub fn render_confirm_revert_share_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
    hits: &mut HitMap,
) {
    let s = i18n::strings(state.language);
    let w = 560;
    let frame = ConfirmFrame {
        size: (w, 246),
        icon: Icon::Refresh,
        icon_colour: C_WARN,
        icon_disc: rgba(255, 183, 77, 42),
        title: s.modal_revert_title,
        subtitle: s.modal_revert_subtitle,
        confirm_label: s.action_revert,
        confirm_hint: s.action_revert,
    };
    let (dx, dy) = draw_confirm_frame(gfx, th, hits, state.language, &frame);

    if let Some(share) = state.shares.get(share_idx) {
        let title = s.fmt_share_title(&share.label, &share.folder_id);
        let (added, restored) = (s.modal_revert_added_hint, s.modal_revert_restored_hint);
        let line_y = |i: i32| dy + 104 + i * 22;
        draw_confirm_lines(
            gfx,
            dx + CONFIRM_PAD_X,
            w - 2 * CONFIRM_PAD_X,
            &[
                (line_y(0), 16, FontWeight::Medium, th.text, &title),
                (
                    line_y(1),
                    13,
                    FontWeight::Regular,
                    th.accent_hi,
                    &share.target_path,
                ),
                (line_y(2), 14, FontWeight::Regular, C_WARN, added),
                (line_y(3), 14, FontWeight::Regular, th.dim, restored),
            ],
        );
    }
}
