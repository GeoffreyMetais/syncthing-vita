//! Modal Dialog: a remote device offers to share a folder (`FolderOffer`), asking for consent.

use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::theme::{folder_offer_modal_rect, folder_offer_slot_x, UiTheme};
use crate::ui::widgets::{draw_action, draw_hints};

/// Renders the `FolderOffer` modal for `state.folder_offers[offer_idx]`.
pub fn render_folder_offer_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState, offer_idx: usize) {
    let Some(offer) = state.folder_offers.get(offer_idx) else {
        return;
    };
    gfx.dim(180);
    let (dx, dy, w, h) = folder_offer_modal_rect();
    let (dx, dy, w, h) = (dx as i32, dy as i32, w as i32, h as i32);

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Folder,
        (lx + 20) as f32,
        (dy + 40) as f32,
        22.0,
        th.accent,
    );
    gfx.text(
        lx + 52,
        dy + 31,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        "New shared folder",
    );
    let device = state
        .devices
        .iter()
        .find(|d| d.device_id.matches_peer(&offer.device_id))
        .map(|d| format!("{} ({})", d.name, d.device_id.short()))
        .unwrap_or_else(|| format!("Device {}", offer.device_id.short()));
    gfx.text_fit(
        lx + 52,
        dy + 54,
        w - 108,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &format!("{} wants to share a folder with this PS Vita", device),
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    gfx.text_fit(
        lx,
        dy + 110,
        w - 56,
        17,
        FontWeight::Medium,
        th.text,
        Align::Left,
        &format!("{}  ·  {}", offer.label, offer.folder_id),
    );
    gfx.text_fit(
        lx,
        dy + 138,
        w - 56,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "Accept to pick a folder on ux0:. Nothing syncs until you enable the share.",
    );
    let pending = state.folder_offers.len();
    if pending > 1 {
        gfx.text(
            lx,
            dy + 164,
            13,
            FontWeight::Regular,
            th.faint,
            Align::Left,
            &format!("{} more offer(s) waiting", pending - 1),
        );
    }

    let cy = dy + h - 34;
    draw_action(
        gfx,
        th,
        folder_offer_slot_x(0) as i32,
        cy,
        PadButton::Square,
        "Decline",
    );
    draw_action(
        gfx,
        th,
        folder_offer_slot_x(1) as i32,
        cy,
        PadButton::Circle,
        "Later",
    );
    draw_action(
        gfx,
        th,
        folder_offer_slot_x(2) as i32,
        cy,
        PadButton::Cross,
        "Accept",
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Cross, None, "Accept share"),
            (PadButton::Square, None, "Decline"),
            (PadButton::Circle, None, "Later"),
        ],
    );
}
