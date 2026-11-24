//! Frame shared by the confirmation dialogs (`ConfirmQuit`, `ConfirmDeleteShare`,
//! `ConfirmRevertShare`): dimmed background, card, icon + title header, `Circle · Cancel` and
//! `Cross · <confirm>` buttons and hints. Each dialog only draws its own body lines.

use crate::domain::interaction::{HitMap, PadKey};
use crate::domain::Language;
use crate::ui::gfx::{Align, Color, FontWeight, Gfx, Icon, PadButton};
use crate::ui::i18n;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{
    action_width, draw_modal_card, draw_modal_header, draw_modal_separator, draw_press_button,
    draw_press_hints, ModalHeader, MODAL_HEADER_TEXT_DX,
};

/// Horizontal padding inside a confirmation dialog.
pub use crate::ui::widgets::MODAL_PAD_X as CONFIRM_PAD_X;
/// Offset of the title / subtitle from the left padding (after the icon).
const HEADER_TEXT_DX: i32 = MODAL_HEADER_TEXT_DX;

/// What distinguishes one confirmation dialog from another.
pub struct ConfirmFrame<'a> {
    pub size: (i32, i32),
    pub icon: Icon,
    /// Icon colour and its translucent disc.
    pub icon_colour: Color,
    pub icon_disc: Color,
    pub title: &'a str,
    pub subtitle: &'a str,
    /// Label of the `Cross` button, and of its entry in the hint bar.
    pub confirm_label: &'a str,
    pub confirm_hint: &'a str,
}

/// Draws the frame of a confirmation dialog and declares its buttons (`Cross` confirms,
/// `Circle` or a touch outside cancels). Returns the top-left corner of the card, from which
/// the caller lays out the body (between `y + 76` and the buttons at `y + h - 34`).
pub fn draw_confirm_frame(
    gfx: &mut Gfx,
    th: &UiTheme,
    hits: &mut HitMap,
    language: Language,
    frame: &ConfirmFrame<'_>,
) -> (i32, i32) {
    let s = i18n::strings(language);
    let (w, h) = frame.size;
    let (dx, dy) = draw_modal_card(
        gfx,
        th,
        hits,
        w,
        h,
        180,
        Some(PadKey::Circle),
        Some(PadKey::Circle),
    );

    let header_max_w = w - 2 * CONFIRM_PAD_X - HEADER_TEXT_DX;
    let header = ModalHeader::new(
        frame.icon,
        22.0,
        frame.icon_colour,
        frame.icon_disc,
        frame.title,
    )
    .subtitle(frame.subtitle, th.dim);
    draw_modal_header(gfx, th, dx, dy, header_max_w, &header);
    draw_modal_separator(gfx, th, dx, dy, w, 76);

    let w_confirm = action_width(gfx, PadButton::Cross, frame.confirm_label);
    let w_cancel = action_width(gfx, PadButton::Circle, s.action_cancel);
    let confirm_x = dx + w - CONFIRM_PAD_X - w_confirm;
    let buttons = [
        (
            confirm_x - 12 - w_cancel,
            PadButton::Circle,
            s.action_cancel,
        ),
        (confirm_x, PadButton::Cross, frame.confirm_label),
    ];
    for (x, button, label) in buttons {
        draw_press_button(gfx, th, hits, x, dy + h - 34, button, label);
    }
    draw_press_hints(
        gfx,
        th,
        hits,
        &[
            (PadButton::Cross, None, frame.confirm_hint),
            (PadButton::Circle, None, s.action_cancel),
        ],
    );
    (dx, dy)
}

/// Draws the body lines of a confirmation dialog, one `(y, size, weight, colour, text)` each,
/// fitted to the card's inner width.
pub fn draw_confirm_lines(
    gfx: &mut Gfx,
    x: i32,
    max_w: i32,
    lines: &[(i32, i32, FontWeight, Color, &str)],
) {
    for &(y, size, weight, colour, text) in lines {
        gfx.text_fit(x, y, max_w, size, weight, colour, Align::Left, text);
    }
}
