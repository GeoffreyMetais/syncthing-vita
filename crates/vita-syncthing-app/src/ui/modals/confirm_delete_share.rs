//! Modal Dialog: Confirmation before deleting a share (`Square` on the `Shares` tab).

use crate::domain::AppState;
use crate::ui::gfx::{rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::theme::{UiTheme, C_BAD, HINT_Y};
use crate::ui::widgets::{action_width, draw_action, draw_hints};

/// Renders the `ConfirmDeleteShare` modal for `shares[share_idx]`.
pub fn render_confirm_delete_share_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
) {
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
        "Delete this share?",
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "The folder stops synchronizing with all linked devices",
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
        gfx.text_fit(
            lx,
            dy + 110,
            w - 56,
            16,
            FontWeight::Medium,
            th.text,
            Align::Left,
            &format!("{}  ·  {}", share.label, share.folder_id),
        );
        gfx.text_fit(
            lx,
            dy + 136,
            w - 56,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            &format!("Files in {} are kept on the PS Vita.", share.target_path),
        );
        gfx.text_fit(
            lx,
            dy + 158,
            w - 56,
            13,
            FontWeight::Regular,
            th.faint,
            Align::Left,
            "Peers' offers of it are declined (undo in Settings · Clear Declined).",
        );
    }

    let w_delete = action_width(gfx, PadButton::Cross, "Delete");
    let w_cancel = action_width(gfx, PadButton::Circle, "Cancel");
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_delete - 12 - w_cancel,
        dy + h - 34,
        PadButton::Circle,
        "Cancel",
    );
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_delete,
        dy + h - 34,
        PadButton::Cross,
        "Delete",
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Cross, None, "Confirm Delete"),
            (PadButton::Circle, None, "Cancel"),
        ],
    );
}
