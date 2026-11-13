//! Modal Dialog: Confirmation before reverting a read-only share's local changes
//! (`START` in `ManageDevices`).

use crate::domain::interaction::{HitMap, PadKey};
use crate::domain::AppState;
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_WARN, HINT_Y};
use crate::ui::widgets::{action_width, declare_modal, draw_press_button, draw_press_hints};

/// Renders the `ConfirmRevertShare` modal for `shares[share_idx]`.
pub fn render_confirm_revert_share_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
    hits: &mut HitMap,
) {
    let s = strings::strings(state.language);
    gfx.dim(180);
    let w = 560;
    let h = 246;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    declare_modal(
        hits,
        (dx, dy, w, h),
        Some(PadKey::Circle),
        Some(PadKey::Circle),
    );
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle(
        (lx + 20) as f32,
        (dy + 40) as f32,
        20.0,
        rgba(255, 183, 77, 42),
    );
    gfx.icon(
        Icon::Refresh,
        (lx + 20) as f32,
        (dy + 40) as f32,
        22.0,
        C_WARN,
    );
    gfx.text_fit(
        lx + 52,
        dy + 31,
        w - 108,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        s.modal_revert_title,
    );
    gfx.text_fit(
        lx + 52,
        dy + 54,
        w - 108,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_revert_subtitle,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    if let Some(share) = state.shares.get(share_idx) {
        let title = s.fmt_share_title(&share.label, &share.folder_id);
        let lines = [
            (16, FontWeight::Medium, th.text, title.as_str()),
            (
                13,
                FontWeight::Regular,
                th.accent_hi,
                share.target_path.as_str(),
            ),
            (14, FontWeight::Regular, C_WARN, s.modal_revert_added_hint),
            (
                14,
                FontWeight::Regular,
                th.dim,
                s.modal_revert_restored_hint,
            ),
        ];
        for (i, (size, weight, col, text)) in lines.into_iter().enumerate() {
            gfx.text_fit(
                lx,
                dy + 104 + (i as i32) * 22,
                w - 56,
                size,
                weight,
                col,
                Align::Left,
                text,
            );
        }
    }

    let w_revert = action_width(gfx, PadButton::Cross, s.action_revert);
    let w_cancel = action_width(gfx, PadButton::Circle, s.action_cancel);
    draw_press_button(
        gfx,
        th,
        hits,
        dx + w - 28 - w_revert - 12 - w_cancel,
        dy + h - 34,
        PadButton::Circle,
        s.action_cancel,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        dx + w - 28 - w_revert,
        dy + h - 34,
        PadButton::Cross,
        s.action_revert,
    );

    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::Cross, None, s.action_revert),
            (PadButton::Circle, None, s.action_cancel),
        ],
    );
}
