//! Modal Dialog: Create a new share on PS Vita (from template or custom `ux0:` folder).

use crate::domain::interaction::{HitMap, PadKey, UiAction};
use crate::domain::{AppState, NEW_SHARE_TEMPLATES};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n;
use crate::ui::theme::{UiTheme, C_GOOD};
use crate::ui::widgets::{
    declare_touch, draw_modal_card, draw_modal_header, draw_modal_separator, draw_press_hints,
    ModalHeader,
};

/// Height of the touch target of a template row.
const TEMPLATE_ROW_H: i32 = 44;

/// Modal Dialog 1: Create a new share on PS Vita (from template or custom `ux0:` folder).
pub fn render_create_share_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    selected_idx: usize,
    hits: &mut HitMap,
) {
    let s = i18n::strings(state.language);
    let w = 680;
    let h = 408;
    let (dx, dy) = draw_modal_card(gfx, th, hits, w, h, 175, Some(PadKey::Circle), None);
    let lx = draw_modal_header(
        gfx,
        th,
        dx,
        dy,
        w - 108,
        &ModalHeader::new(
            Icon::Plus,
            22.0,
            th.accent_hi,
            th.accent_bg,
            s.modal_create_share_title,
        )
        .title_pos(32, 21)
        .subtitle(s.modal_create_share_subtitle, 54, 14, th.dim),
    );
    draw_modal_separator(gfx, th, dx, dy, w, 76);

    for (i, &(fid, label, path)) in NEW_SHARE_TEMPLATES.iter().enumerate() {
        let ry = dy + 90 + (i as i32) * 50;
        let cy = ry + 21;
        // Touching a template picks it.
        declare_touch(hits, dx, ry, w, TEMPLATE_ROW_H, UiAction::ModalRow(i));
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
            s.modal_create_share_browse_sub.to_owned()
        } else {
            s.fmt_template_sub(fid, path)
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

    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::DpadUd, None, s.hint_select),
            (PadButton::Cross, None, s.hint_create_pick_devices),
            (PadButton::Circle, None, s.action_cancel),
        ],
    );
}
