//! Modal Dialog: an unknown remote device tried to connect (`DeviceOffer`), asking for consent.

use crate::domain::interaction::HitMap;
use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n;
use crate::ui::theme::{folder_offer_modal_rect, folder_offer_slot_x, UiTheme};
use crate::ui::widgets::{
    draw_modal_card, draw_modal_header, draw_modal_separator, draw_press_button, draw_press_hints,
    ModalHeader,
};

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
    let (_, _, w, h) = folder_offer_modal_rect();
    let (w, h) = (w as i32, h as i32);
    let (dx, dy) = draw_modal_card(gfx, th, hits, w, h, 180, None, None);

    let source = if offer.via_relay {
        s.modal_device_offer_via_relay
    } else if !offer.address.is_empty() {
        offer.address.as_str()
    } else {
        s.modal_device_offer_via_relay
    };
    let sub = s.fmt_device_offer_sub(source);
    let lx = draw_modal_header(
        gfx,
        th,
        dx,
        dy,
        w - 108,
        &ModalHeader::new(
            Icon::Wifi,
            22.0,
            th.accent,
            th.accent_bg,
            s.modal_device_offer_title,
        )
        .subtitle(&sub, 54, 14, th.dim),
    );
    draw_modal_separator(gfx, th, dx, dy, w, 76);

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
