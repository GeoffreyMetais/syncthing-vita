//! Frame shared by the confirmation dialogs (`ConfirmQuit`, `ConfirmDeleteShare`,
//! `ConfirmRevertShare`): dimmed background, card, icon + title header, `Circle · Cancel` and
//! `Cross · <confirm>` buttons and hints. Each dialog only draws its own body lines.

use crate::domain::interaction::{HitMap, PadKey};
use crate::domain::Language;
use crate::ui::gfx::{Align, Color, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::i18n;
use crate::ui::theme::{UiTheme, HINT_Y};
use crate::ui::widgets::{action_width, declare_modal, draw_press_button, draw_press_hints};

/// Horizontal padding inside a confirmation dialog.
pub const CONFIRM_PAD_X: i32 = 28;
/// Offset of the title / subtitle from the left padding (after the icon).
const HEADER_TEXT_DX: i32 = 52;

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
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.dim(180);
    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    declare_modal(
        hits,
        (dx, dy, w, h),
        Some(PadKey::Circle),
        Some(PadKey::Circle),
    );
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + CONFIRM_PAD_X;
    let header_max_w = w - 2 * CONFIRM_PAD_X - HEADER_TEXT_DX;
    let (icon_x, icon_y) = ((lx + 20) as f32, (dy + 40) as f32);
    gfx.circle(icon_x, icon_y, 20.0, frame.icon_disc);
    gfx.icon(frame.icon, icon_x, icon_y, 22.0, frame.icon_colour);
    let header = [
        (dy + 31, 21, FontWeight::Bold, th.text, frame.title),
        (dy + 54, 14, FontWeight::Regular, th.dim, frame.subtitle),
    ];
    for (y, size, weight, colour, text) in header {
        gfx.text_fit(
            lx + HEADER_TEXT_DX,
            y,
            header_max_w,
            size,
            weight,
            colour,
            Align::Left,
            text,
        );
    }
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

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
