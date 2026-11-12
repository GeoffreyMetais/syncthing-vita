//! Tab 2 (`Shared Folders` / `Shares`): List of configured Syncthing shares, `+ New Share` button,
//! and per-share actions (`Devices`, `Delete`, `Folder`).
//!
//! Every interactive element is declared in the frame's `HitMap` where it is drawn; input is
//! handled from these declarations by `application::SharesController`.

use crate::domain::{AppState, FocusSlot, HitMap, Hotspot, Rect, UiAction};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, CONTENT_R, CONTENT_W, CONTENT_X, CONTENT_Y, C_GOOD, C_WARN};
use crate::ui::widgets::{
    action_width, draw_action_hotspot, draw_card, draw_chip, draw_hint_bar, draw_page_title, Hint,
};

/// Shares visible on the page (the list does not scroll yet).
const VISIBLE_SHARES: usize = 4;
/// Height of a share card and vertical distance between two cards.
const SHARE_CARD_H: i32 = 90;
const SHARE_CARD_STEP: i32 = 98;
/// Vertical centre of the `+ New Share` button.
const NEW_SHARE_CY: i32 = 46;

/// Renders Tab 2: **Shared Folders** (`Shares`).
pub fn render_shares_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    draw_page_title(gfx, th, s.shares_title, None);

    // Top-right "+ New Share" action button (Circle)
    let w_new = action_width(gfx, PadButton::Circle, s.action_new_share);
    draw_action_hotspot(
        gfx,
        th,
        hits,
        CONTENT_R - w_new,
        NEW_SHARE_CY,
        PadButton::Circle,
        s.action_new_share,
        UiAction::NewShare,
        FocusSlot::Header,
        state.is_focused(FocusSlot::Header),
    );

    if state.shares.is_empty() {
        draw_card(gfx, th, CONTENT_X, CONTENT_Y, CONTENT_W, 150, false);
        gfx.text(
            CONTENT_X + 28,
            CONTENT_Y + 46,
            18,
            FontWeight::Medium,
            th.text,
            Align::Left,
            s.msg_no_shares,
        );
        gfx.text(
            CONTENT_X + 28,
            CONTENT_Y + 78,
            15,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            s.msg_no_shares_hint_1,
        );
        gfx.text(
            CONTENT_X + 28,
            CONTENT_Y + 104,
            15,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            s.msg_no_shares_hint_2,
        );
    } else {
        for (i, share) in state.shares.iter().take(VISIBLE_SHARES).enumerate() {
            let ry = CONTENT_Y + (i as i32) * SHARE_CARD_STEP;
            let is_sel = i == state.selected_share_idx;
            let devs_summary = state.share_devices_summary(share);

            draw_card(gfx, th, CONTENT_X, ry, CONTENT_W, SHARE_CARD_H, is_sel);
            hits.add(Hotspot {
                rect: Rect::new(CONTENT_X, ry, CONTENT_W, SHARE_CARD_H),
                action: UiAction::SelectShare(i),
                focus: None,
                shortcut: None,
            });

            let lx = CONTENT_X + 18;
            gfx.rrect(lx, ry + 19, 52, 52, 14, th.accent_bg);
            gfx.icon(
                Icon::Folder,
                (lx + 26) as f32,
                (ry + 45) as f32,
                28.0,
                if share.enabled { th.accent_hi } else { th.dim },
            );

            // Action buttons on the right: [X Devices] [SQ Delete] [TRI Folder]
            let w_folder = action_width(gfx, PadButton::Triangle, s.action_folder);
            let w_toggle = action_width(gfx, PadButton::Square, s.action_delete);
            let w_devs = action_width(gfx, PadButton::Cross, s.action_devices);

            let bx_folder = CONTENT_R - 14 - w_folder;
            let bx_toggle = bx_folder - 8 - w_toggle;
            let bx_devs = bx_toggle - 8 - w_devs;

            // Left to right: the column order is the D-pad order within the row.
            let buttons = [
                (bx_devs, PadButton::Cross, s.action_devices, UiAction::ManageDevices(i)),
                (bx_toggle, PadButton::Square, s.action_delete, UiAction::DeleteShare(i)),
                (bx_folder, PadButton::Triangle, s.action_folder, UiAction::ChangeShareFolder(i)),
            ];
            for (col, (bx, pad, label, action)) in buttons.into_iter().enumerate() {
                draw_action_hotspot(
                    gfx,
                    th,
                    hits,
                    bx,
                    ry + 56,
                    pad,
                    label,
                    action,
                    FocusSlot::RowAction { row: i, col },
                    state.is_focused(FocusSlot::RowAction { row: i, col }),
                );
            }

            let status_col = if share.enabled { C_GOOD } else { C_WARN };
            let status_txt = if share.enabled { s.status_active } else { s.status_paused };
            let chip_w = draw_chip(gfx, CONTENT_R - 14, ry + 22, status_txt, status_col);

            let tx = lx + 66;
            let title = s.fmt_share_title(&share.label, &share.folder_id);
            gfx.text_fit(
                tx,
                ry + 22,
                CONTENT_R - 24 - chip_w - tx,
                17,
                FontWeight::Bold,
                th.text,
                Align::Left,
                &title,
            );

            let max_sub_w = bx_devs - tx - 12;
            gfx.text_fit(
                tx,
                ry + 46,
                max_sub_w,
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
            gfx.text_fit(
                tx,
                ry + 68,
                max_sub_w,
                13,
                FontWeight::Regular,
                dev_col,
                Align::Left,
                &s.text(&devs_summary),
            );
        }
    }

    let selected = state.selected_share_idx;
    let has_selection = selected < state.shares.len();
    let row_action = |action: UiAction| has_selection.then_some(action);
    draw_hint_bar(
        gfx,
        th,
        hits,
        &[
            Hint::info(PadButton::Dpad, Some(PadButton::Cross), s.hint_navigate),
            Hint::action(PadButton::Circle, s.action_new_share, UiAction::NewShare),
            Hint {
                action: row_action(UiAction::DeleteShare(selected)),
                ..Hint::info(PadButton::Square, None, s.action_delete)
            },
            Hint {
                action: row_action(UiAction::ChangeShareFolder(selected)),
                ..Hint::info(PadButton::Triangle, None, s.hint_folder_path)
            },
            Hint::info(PadButton::L, Some(PadButton::R), s.hint_tabs),
        ],
    );
}
