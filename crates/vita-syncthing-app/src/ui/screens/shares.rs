//! Share list of the `Overview` page: one compact card per configured Syncthing share with its
//! `Devices`, `Delete` and `Folder` actions. The list scrolls to keep the selected share visible.
//!
//! Every interactive element is declared in the frame's `HitMap` where it is drawn; input is
//! handled from these declarations by `application::SharesController`.

use crate::domain::{ActiveTab, AppState, FocusSlot, HitMap, Hotspot, Rect, UiAction};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, CONTENT_R, CONTENT_W, CONTENT_X, C_GOOD, C_WARN};
use crate::ui::widgets::{action_width, draw_action_hotspot, draw_card, draw_chip, draw_scrollbar};

/// Shares visible at once; the list scrolls with the selection beyond that.
const VISIBLE_SHARES: usize = 4;
/// Height of a share card and vertical distance between two cards.
const SHARE_CARD_H: i32 = 64;
const SHARE_CARD_STEP: i32 = 72;
/// Height of the card shown when no share is configured.
const EMPTY_CARD_H: i32 = 118;

/// Draws the share list from `top_y` down to the bottom of the content area.
pub fn draw_share_list(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    hits: &mut HitMap,
    top_y: i32,
) {
    if state.shares.is_empty() {
        draw_empty_list(gfx, th, state, top_y);
        return;
    }
    let count = state.shares.len();
    let first = first_visible_share(state.selected_share_idx, count);
    let last = (first + VISIBLE_SHARES).min(count);
    for i in first..last {
        let ry = top_y + ((i - first) as i32) * SHARE_CARD_STEP;
        draw_share_card(gfx, th, state, hits, i, ry);
    }
    let list_h = (VISIBLE_SHARES as i32 - 1) * SHARE_CARD_STEP + SHARE_CARD_H;
    draw_scrollbar(
        gfx,
        th,
        CONTENT_R + 8,
        top_y,
        list_h,
        first,
        VISIBLE_SHARES,
        count,
    );
}

/// First share drawn so that `selected` is visible.
fn first_visible_share(selected: usize, count: usize) -> usize {
    let max_first = count.saturating_sub(VISIBLE_SHARES);
    selected
        .min(count.saturating_sub(1))
        .saturating_sub(VISIBLE_SHARES - 1)
        .min(max_first)
}

fn draw_empty_list(gfx: &mut Gfx, th: &UiTheme, state: &AppState, top_y: i32) {
    let s = strings::strings(state.language);
    draw_card(gfx, th, CONTENT_X, top_y, CONTENT_W, EMPTY_CARD_H, false);
    let lines = [
        (34, 17, FontWeight::Medium, th.text, s.msg_no_shares),
        (66, 14, FontWeight::Regular, th.dim, s.msg_no_shares_hint_1),
        (90, 14, FontWeight::Regular, th.dim, s.msg_no_shares_hint_2),
    ];
    for (dy, size, weight, col, text) in lines {
        gfx.text_fit(
            CONTENT_X + 24,
            top_y + dy,
            CONTENT_W - 48,
            size,
            weight,
            col,
            Align::Left,
            text,
        );
    }
}

fn draw_share_card(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    hits: &mut HitMap,
    i: usize,
    ry: i32,
) {
    let s = strings::strings(state.language);
    let share = &state.shares[i];
    // The selected card is not highlighted while the device card above holds the focus.
    let is_sel = i == state.selected_share_idx && !state.is_header_focused(ActiveTab::Dashboard);

    draw_card(gfx, th, CONTENT_X, ry, CONTENT_W, SHARE_CARD_H, is_sel);
    hits.add(Hotspot {
        rect: Rect::new(CONTENT_X, ry, CONTENT_W, SHARE_CARD_H),
        action: UiAction::SelectShare(i),
        focus: None,
        shortcut: None,
    });

    let lx = CONTENT_X + 14;
    gfx.rrect(lx, ry + 10, 44, 44, 12, th.accent_bg);
    gfx.icon(
        Icon::Folder,
        (lx + 22) as f32,
        (ry + 32) as f32,
        24.0,
        if share.enabled { th.accent_hi } else { th.dim },
    );

    // Action buttons on the right: [X Devices] [SQ Delete] [X Folder]. `Triangle` is the
    // page's `New Share`, so `Folder` is reached with the D-pad (`Cross`) or by touch.
    let w_folder = action_width(gfx, PadButton::Cross, s.action_folder);
    let w_delete = action_width(gfx, PadButton::Square, s.action_delete);
    let w_devs = action_width(gfx, PadButton::Cross, s.action_devices);
    let bx_folder = CONTENT_R - 12 - w_folder;
    let bx_delete = bx_folder - 8 - w_delete;
    let bx_devs = bx_delete - 8 - w_devs;

    // Left to right: the column order is the D-pad order within the row.
    let buttons = [
        (
            bx_devs,
            PadButton::Cross,
            s.action_devices,
            UiAction::ManageDevices(i),
        ),
        (
            bx_delete,
            PadButton::Square,
            s.action_delete,
            UiAction::DeleteShare(i),
        ),
        (
            bx_folder,
            PadButton::Cross,
            s.action_folder,
            UiAction::ChangeShareFolder(i),
        ),
    ];
    for (col, (bx, pad, label, action)) in buttons.into_iter().enumerate() {
        let slot = FocusSlot::RowAction { row: i, col };
        draw_action_hotspot(
            gfx,
            th,
            hits,
            bx,
            ry + SHARE_CARD_H / 2,
            pad,
            label,
            action,
            slot,
            state.is_focused(slot),
        );
    }

    // Line 1: title, then the status chip right after it.
    let tx = lx + 58;
    let max_w = bx_devs - 16 - tx;
    let (status_txt, status_col) = if share.enabled {
        (s.status_active, C_GOOD)
    } else {
        (s.status_paused, C_WARN)
    };
    let chip_w = gfx.text_width(13, FontWeight::Medium, status_txt) + 22;
    let title = s.fmt_share_title(&share.label, &share.folder_id);
    let title_w = gfx.text_fit(
        tx,
        ry + 21,
        max_w - chip_w - 10,
        16,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &title,
    );
    draw_chip(
        gfx,
        tx + title_w + 10 + chip_w,
        ry + 21,
        status_txt,
        status_col,
    );

    // Line 2: target path, then the linked devices. The path keeps its full width unless the
    // devices need more than what is left (they get at least 40% of the line).
    let summary = state.share_devices_summary(share);
    let devices = s.text(&summary);
    let devices_w = gfx.text_width(13, FontWeight::Regular, &devices);
    let path_max = max_w - 14 - devices_w.min(max_w * 2 / 5);
    let path_w = gfx.text_fit(
        tx,
        ry + 45,
        path_max,
        13,
        FontWeight::Medium,
        th.accent_hi,
        Align::Left,
        &share.target_path,
    );
    let dev_col = if share.shared_devices.is_empty() {
        C_WARN
    } else {
        th.dim
    };
    let dev_x = tx + path_w + 14;
    gfx.text_fit(
        dev_x,
        ry + 45,
        tx + max_w - dev_x,
        13,
        FontWeight::Regular,
        dev_col,
        Align::Left,
        &devices,
    );
}
