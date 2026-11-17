//! `Settings` card of the background sync plugin: section header and buttons on the top row
//! (`Install`, or `Enable / Disable` + `Uninstall` once installed), then the status chip and
//! what the status means (reboot needed, paused while this app is open...) over the card's
//! full width.

use crate::domain::{AppState, FocusSlot, HitMap, PluginStatus, UiAction, SETTINGS_ROW_PLUGIN};
use crate::ui::gfx::{Align, Color, FontWeight, Gfx, PadButton};
use crate::ui::strings;
use crate::ui::theme::{
    UiTheme, CONTENT_R, CONTENT_W, CONTENT_X, C_GOOD, C_WARN, SETTINGS_PLUGIN_B, SETTINGS_PLUGIN_Y,
};
use crate::ui::widgets::{action_width, draw_action_hotspot, draw_card, draw_chip};

/// Vertical centre of the header row (section title and buttons), from the card top.
const HEADER_DY: i32 = 22;
/// Vertical centre of the status row (chip and description), from the card top.
const STATUS_DY: i32 = 48;
/// Font size of the description.
pub const PLUGIN_DESC_SIZE: i32 = 13;
/// Gap between the status chip and the description.
const CHIP_GAP: i32 = 10;

/// Where the card's texts go, given the labels of the current language (also used by the
/// geometry tests: nothing may be cut).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginCardLayout {
    /// Left `x` of each button (left to right) and its width.
    pub buttons: [(i32, i32); 2],
    pub button_count: usize,
    /// Room for the section title, left of the first button.
    pub title_max_w: i32,
    /// Width of the status chip.
    pub chip_w: i32,
    /// Left `x` of the description and the room it has.
    pub desc_x: i32,
    pub desc_max_w: i32,
}

/// Buttons of the card for `state`, left to right.
fn buttons(state: &AppState) -> Vec<(&'static str, UiAction)> {
    let p = strings::strings(state.language).plugin;
    if state.plugin.status.is_installed() {
        let toggle = if state.plugin.enabled {
            p.action_disable
        } else {
            p.action_enable
        };
        vec![
            (toggle, UiAction::TogglePluginEnabled),
            (p.action_uninstall, UiAction::UninstallPlugin),
        ]
    } else {
        vec![(p.action_install, UiAction::InstallPlugin)]
    }
}

/// Lays the card out for `state`.
pub fn plugin_card_layout(gfx: &mut Gfx, state: &AppState) -> PluginCardLayout {
    let p = strings::strings(state.language).plugin;
    let lx = CONTENT_X + 24;
    let labels = buttons(state);
    let mut out = PluginCardLayout {
        buttons: [(0, 0); 2],
        button_count: labels.len(),
        title_max_w: 0,
        chip_w: 0,
        desc_x: 0,
        desc_max_w: 0,
    };
    let mut x = CONTENT_R - 20;
    for (i, (label, _)) in labels.iter().enumerate().rev() {
        let w = action_width(gfx, PadButton::Cross, label);
        x -= w;
        out.buttons[i] = (x, w);
        x -= 8;
    }
    out.title_max_w = out.buttons[0].0 - 16 - lx;
    let status = p.status(state.plugin.status);
    out.chip_w = gfx.text_width(13, FontWeight::Medium, status) + 22;
    out.desc_x = lx + out.chip_w + CHIP_GAP;
    out.desc_max_w = CONTENT_R - 24 - out.desc_x;
    out
}

/// Chip colour of `status`.
fn status_colour(th: &UiTheme, status: PluginStatus) -> Color {
    match status {
        PluginStatus::NotInstalled => th.dim,
        PluginStatus::InstalledNeedsReboot => C_WARN,
        PluginStatus::Enabled => C_GOOD,
        PluginStatus::Disabled => th.faint,
    }
}

/// Text of the description for `state` (also used by the geometry tests).
pub fn plugin_description(state: &AppState) -> &'static str {
    strings::strings(state.language)
        .plugin
        .description(state.plugin.status, state.plugin.unload_pending())
}

/// Draws the card and declares its buttons (D-pad row `SETTINGS_ROW_PLUGIN`).
pub fn render_plugin_card(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let p = strings::strings(state.language).plugin;
    let y = SETTINGS_PLUGIN_Y;
    let h = SETTINGS_PLUGIN_B - SETTINGS_PLUGIN_Y;
    draw_card(gfx, th, CONTENT_X, y, CONTENT_W, h, false);
    let layout = plugin_card_layout(gfx, state);
    let lx = CONTENT_X + 24;
    gfx.text_fit(
        lx,
        y + HEADER_DY,
        layout.title_max_w,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        p.section,
    );
    for (col, (label, action)) in buttons(state).into_iter().enumerate() {
        let slot = FocusSlot::RowAction {
            row: SETTINGS_ROW_PLUGIN,
            col,
        };
        draw_action_hotspot(
            gfx,
            th,
            hits,
            layout.buttons[col].0,
            y + HEADER_DY,
            PadButton::Cross,
            label,
            action,
            slot,
            state.is_focused(slot),
        );
    }

    let status = state.plugin.status;
    draw_chip(
        gfx,
        lx + layout.chip_w,
        y + STATUS_DY,
        p.status(status),
        status_colour(th, status),
    );
    let desc_colour =
        if status == PluginStatus::InstalledNeedsReboot || state.plugin.unload_pending() {
            C_WARN
        } else {
            th.dim
        };
    gfx.text_fit(
        layout.desc_x,
        y + STATUS_DY,
        layout.desc_max_w,
        PLUGIN_DESC_SIZE,
        FontWeight::Regular,
        desc_colour,
        Align::Left,
        plugin_description(state),
    );
}
