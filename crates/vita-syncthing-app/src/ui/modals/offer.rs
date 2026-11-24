//! Modal Dialog: consent for an incoming offer — a folder shared by a known remote device
//! (`FolderOffer`) or an unknown device trying to connect (`DeviceOffer`). Both share the
//! same card, header, body layout and `Decline` / `Later` / `Accept` buttons; only the icon,
//! texts and the first body line differ.

use crate::domain::interaction::HitMap;
use crate::domain::{AppState, ModalDialog};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n::messages::fill;
use crate::ui::i18n;
use crate::ui::theme::{folder_offer_modal_rect, folder_offer_slot_x, UiTheme};
use crate::ui::widgets::{
    draw_modal_card, draw_modal_header, draw_modal_separator, draw_press_button, draw_press_hints,
    ModalHeader,
};

/// Everything that distinguishes a folder offer from a device offer on screen.
struct OfferView<'a> {
    icon: Icon,
    title: &'a str,
    /// Requester (folder) or connection source (device) line under the title.
    sub: String,
    /// Offered share title (folder) or full Device ID (device).
    body: String,
    body_size: i32,
    hint: &'a str,
    accept_hint: &'a str,
    pending: usize,
}

/// Renders the open offer modal (`FolderOffer` or `DeviceOffer`).
pub fn render_offer_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let (folder, offer_idx) = match state.modal {
        ModalDialog::FolderOffer { offer_idx } => (true, offer_idx),
        ModalDialog::DeviceOffer { offer_idx } => (false, offer_idx),
        _ => return,
    };
    let s = i18n::strings(state.language);
    let view = if folder {
        let Some(offer) = state.folder_offers.get(offer_idx) else {
            return;
        };
        let device = state
            .devices
            .iter()
            .find(|d| d.device_id.matches_peer(&offer.device_id))
            .map(|d| format!("{} ({})", d.name, d.device_id.short()))
            .unwrap_or_else(|| {
                fill(
                    s.messages.unknown_device,
                    &[("short", &offer.device_id.short())],
                )
            });
        OfferView {
            icon: Icon::Folder,
            title: s.modal_folder_offer_title,
            sub: s.fmt_folder_offer_sub(&device),
            body: s.fmt_share_title(&offer.label, &offer.folder_id),
            body_size: 17,
            hint: s.modal_folder_offer_hint,
            accept_hint: s.hint_accept_share,
            pending: state.folder_offers.len(),
        }
    } else {
        let Some(offer) = state.device_offers.get(offer_idx) else {
            return;
        };
        let source = if offer.via_relay {
            s.modal_device_offer_via_relay
        } else if !offer.address.is_empty() {
            offer.address.as_str()
        } else {
            s.modal_device_offer_via_relay
        };
        OfferView {
            icon: Icon::Wifi,
            title: s.modal_device_offer_title,
            sub: s.fmt_device_offer_sub(source),
            body: offer.device_id.to_syncthing_string(),
            body_size: 13,
            hint: s.modal_device_offer_hint,
            accept_hint: s.hint_accept_device,
            pending: state.device_offers.len(),
        }
    };

    let (_, _, w, h) = folder_offer_modal_rect();
    let (w, h) = (w as i32, h as i32);
    let (dx, dy) = draw_modal_card(gfx, th, hits, w, h, 180, None, None);
    let lx = draw_modal_header(
        gfx,
        th,
        dx,
        dy,
        w - 108,
        &ModalHeader::new(view.icon, 22.0, th.accent, th.accent_bg, view.title)
            .subtitle(&view.sub, 54, 14, th.dim),
    );
    draw_modal_separator(gfx, th, dx, dy, w, 76);

    gfx.text_fit(
        lx,
        dy + 110,
        w - 56,
        view.body_size,
        FontWeight::Medium,
        th.text,
        Align::Left,
        &view.body,
    );
    gfx.text_fit(
        lx,
        dy + 138,
        w - 56,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        view.hint,
    );
    if view.pending > 1 {
        let waiting_str = s.fmt_offers_waiting(view.pending - 1);
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
            (PadButton::Cross, None, view.accept_hint),
            (PadButton::Square, None, s.action_decline),
            (PadButton::Circle, None, s.action_later),
        ],
    );
}
