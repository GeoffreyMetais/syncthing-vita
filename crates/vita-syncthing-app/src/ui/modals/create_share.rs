//! Modal Dialog: Create a new share on PS Vita (from template or custom `ux0:` folder).

use crate::domain::NEW_SHARE_TEMPLATES;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_GOOD, HINT_Y};
use crate::ui::widgets::draw_hints;

/// Modal Dialog 1: Create a new share on PS Vita (from template or custom `ux0:` folder).
pub fn render_create_share_modal(gfx: &mut Gfx, th: &UiTheme, selected_idx: usize) {
    gfx.dim(175);
    let w = 680;
    let h = 408;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Plus,
        (lx + 20) as f32,
        (dy + 40) as f32,
        22.0,
        th.accent_hi,
    );
    gfx.text(
        lx + 52,
        dy + 32,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        strings::MODAL_CREATE_SHARE_TITLE,
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        strings::MODAL_CREATE_SHARE_SUBTITLE,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    for (i, &(fid, label, path)) in NEW_SHARE_TEMPLATES.iter().enumerate() {
        let ry = dy + 90 + (i as i32) * 50;
        let cy = ry + 21;
        let is_sel = i == selected_idx;

        if is_sel {
            gfx.rrect(lx, ry, w - 56, 44, 10, th.accent_bg);
            gfx.rrect(lx, ry + 9, 4, 26, 2, th.accent);
        }

        let is_browse = fid == "__browse__";
        let ic = if is_browse {
            Icon::Receive
        } else {
            Icon::Folder
        };
        gfx.icon(
            ic,
            (lx + 24) as f32,
            cy as f32,
            22.0,
            if is_browse { C_GOOD } else { th.accent_hi },
        );

        gfx.text(
            lx + 48,
            cy - 7,
            16,
            if is_sel {
                FontWeight::Bold
            } else {
                FontWeight::Medium
            },
            th.text,
            Align::Left,
            label,
        );
        let sub = if is_browse {
            strings::MODAL_CREATE_SHARE_BROWSE_SUB.to_owned()
        } else {
            strings::fmt_template_sub(fid, path)
        };
        gfx.text(
            lx + 48,
            cy + 12,
            13,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            &sub,
        );
    }

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::DpadUd, None, strings::HINT_SELECT),
            (PadButton::Cross, None, strings::HINT_CREATE_PICK_DEVICES),
            (PadButton::Circle, None, strings::ACTION_CANCEL),
        ],
    );
}
