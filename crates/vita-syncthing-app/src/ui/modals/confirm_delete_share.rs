//! Modal Dialog: Confirmation before deleting a share (`Square` on the `Overview` share list).

use crate::domain::interaction::{HitMap, PadKey};
use crate::domain::AppState;
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_BAD, HINT_Y};
use crate::ui::widgets::{action_width, declare_modal, draw_press_button, draw_press_hints};

/// Renders the `ConfirmDeleteShare` modal for `shares[share_idx]`.
pub fn render_confirm_delete_share_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
    hits: &mut HitMap,
) {
    let s = strings::strings(state.language);
    gfx.dim(180);
    let w = 520;
    let h = 236;
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
        s.modal_delete_share_title,
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_delete_share_subtitle,
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
        let files_msg = s.fmt_files_kept_on_vita(&share.target_path);
        gfx.text_fit(
            lx,
            dy + 110,
            w - 56,
            16,
            FontWeight::Medium,
            th.text,
            Align::Left,
            &title,
        );
        gfx.text_fit(
            lx,
            dy + 136,
            w - 56,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            &files_msg,
        );
        gfx.text_fit(
            lx,
            dy + 158,
            w - 56,
            13,
            FontWeight::Regular,
            th.faint,
            Align::Left,
            s.modal_delete_share_decline_hint,
        );
    }

    let w_delete = action_width(gfx, PadButton::Cross, s.action_delete);
    let w_cancel = action_width(gfx, PadButton::Circle, s.action_cancel);
    draw_press_button(
        gfx,
        th,
        hits,
        dx + w - 28 - w_delete - 12 - w_cancel,
        dy + h - 34,
        PadButton::Circle,
        s.action_cancel,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        dx + w - 28 - w_delete,
        dy + h - 34,
        PadButton::Cross,
        s.action_delete,
    );

    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::Cross, None, s.hint_confirm_delete),
            (PadButton::Circle, None, s.action_cancel),
        ],
    );
}
