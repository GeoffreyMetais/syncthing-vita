//! Modal Dialog: Confirmation before installing or uninstalling the background sync plugin
//! (`Settings`): what it does, and that a reboot is needed for it to take effect.

use super::confirm_frame::{draw_confirm_frame, draw_confirm_lines, ConfirmFrame, CONFIRM_PAD_X};
use crate::domain::interaction::HitMap;
use crate::domain::{AppState, PluginRequest};
use crate::ui::gfx::{rgba, FontWeight, Gfx, Icon};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, CONFIRM_PLUGIN_H, CONFIRM_PLUGIN_W, C_BAD, C_WARN};

/// Y of body line `i` of the modal, from its top.
pub const fn confirm_plugin_line_dy(i: i32) -> i32 {
    106 + i * 26
}

/// Renders the `ConfirmPlugin` modal for `request` (`Install` or `Uninstall`).
pub fn render_confirm_plugin_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    request: PluginRequest,
    hits: &mut HitMap,
) {
    let text = strings::strings(state.language).plugin.confirm(request);
    let (icon, colour, disc) = match request {
        PluginRequest::Install => (Icon::Package, th.accent_hi, th.accent_bg),
        _ => (Icon::Close, C_BAD, rgba(239, 83, 80, 40)),
    };
    let frame = ConfirmFrame {
        size: (CONFIRM_PLUGIN_W, CONFIRM_PLUGIN_H),
        icon,
        icon_colour: colour,
        icon_disc: disc,
        title: text.title,
        subtitle: text.subtitle,
        confirm_label: text.confirm,
        confirm_hint: text.confirm,
    };
    let (dx, dy) = draw_confirm_frame(gfx, th, hits, state.language, &frame);
    let line_y = |i: i32| dy + confirm_plugin_line_dy(i);
    draw_confirm_lines(
        gfx,
        dx + CONFIRM_PAD_X,
        CONFIRM_PLUGIN_W - 2 * CONFIRM_PAD_X,
        &[
            (line_y(0), 14, FontWeight::Regular, th.text, text.lines[0]),
            (line_y(1), 14, FontWeight::Regular, th.dim, text.lines[1]),
            (line_y(2), 14, FontWeight::Medium, C_WARN, text.reboot),
        ],
    );
}
