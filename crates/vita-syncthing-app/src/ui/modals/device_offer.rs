//! Modal Dialog: an unknown remote device tried to connect (`DeviceOffer`), asking for consent.

use crate::domain::interaction::HitMap;
use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n;
use crate::ui::theme::{folder_offer_modal_rect, folder_offer_slot_x, UiTheme};
use crate::ui::widgets::{declare_modal, draw_press_button, draw_press_hints};

/// Renders the `DeviceOffer` modal for `state.device_offers[offer_idx]`.
pub fn render_device_offer_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    offer_idx: usize,
    hits: &mut HitMap,
) {
    let Some(offer) = state.device_offers.get(offer_idx) else {
        return;
    };
    let s = i18n::strings(state.language);
    gfx.dim(180);
    let (dx, dy, w, h) = folder_offer_modal_rect();
    let (dx, dy, w, h) = (dx as i32, dy as i32, w as i32, h as i32);

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    declare_modal(hits, (dx, dy, w, h), None, None);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Wifi,
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
        s.modal_device_offer_title,
    );
    let source = if offer.via_relay {
        s.modal_device_offer_via_relay
    } else if !offer.address.is_empty() {
        offer.address.as_str()
    } else {
        s.modal_device_offer_via_relay
    };
    let sub = s.fmt_device_offer_sub(source);
    gfx.text_fit(
        lx + 52,
        dy + 54,
        w - 108,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &sub,
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    let full_id = offer.device_id.to_syncthing_string();
    gfx.text_fit(
        lx,
        dy + 110,
        w - 56,
        13,
        FontWeight::Medium,
        th.text,
        Align::Left,
        &full_id,
    );
    gfx.text_fit(
        lx,
        dy + 138,
        w - 56,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.modal_device_offer_hint,
    );
    let pending = state.device_offers.len();
    if pending > 1 {
        let waiting_str = s.fmt_offers_waiting(pending - 1);
        gfx.text(
            lx,
            dy + 164,
            13,
            FontWeight::Regular,
            th.faint,
            Align::Left,
            &waiting_str,
        );
    }

    let cy = dy + h - 34;
    draw_press_button(
        gfx,
        th,
        hits,
        folder_offer_slot_x(0) as i32,
        cy,
        PadButton::Square,
        s.action_decline,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        folder_offer_slot_x(1) as i32,
        cy,
        PadButton::Circle,
        s.action_later,
    );
    draw_press_button(
        gfx,
        th,
        hits,
        folder_offer_slot_x(2) as i32,
        cy,
        PadButton::Cross,
        s.action_accept,
    );

    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::Cross, None, s.hint_accept_device),
            (PadButton::Square, None, s.action_decline),
            (PadButton::Circle, None, s.action_later),
        ],
    );
}
