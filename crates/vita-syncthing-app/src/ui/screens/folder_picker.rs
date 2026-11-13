//! Tab 3 (`Files` / `FolderPicker`): Interactive `ux0:` directory browser and folder presets.

use crate::domain::{
    AppState, FocusSlot, HitMap, Hotspot, PickerMode, Rect, UiAction, BROWSER_ROWS,
};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::strings;
use crate::ui::theme::{
    UiTheme, BROWSER_STEP, BROWSER_TOP, CONTENT_R, CONTENT_W, CONTENT_X, C_GOOD,
};
use crate::ui::widgets::{
    action_width, draw_action_hotspot, draw_card, draw_chip, draw_hint_bar, draw_page_title,
    draw_scrollbar, Hint,
};

/// Height of a folder row (rows are `BROWSER_STEP` apart).
const BROWSER_ROW_H: i32 = 38;
/// Horizontal margin added left of a row's `Open` chip to form its touch target.
const CHIP_TOUCH_MARGIN: i32 = 16;

/// Renders Tab 3: **Files** (`FolderPicker`).
pub fn render_folder_picker_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let (page_title, share_name) = if state.picker_mode == PickerMode::CreateNewShare {
        (s.picker_title_create, s.picker_subtitle_create.to_owned())
    } else {
        let title_sub = state
            .shares
            .get(state.selected_share_idx)
            .map(|share| s.fmt_picker_target(&share.label, &share.folder_id))
            .unwrap_or_else(|| s.picker_default_share.to_owned());
        (s.picker_title_choose, title_sub)
    };

    draw_page_title(gfx, th, page_title, Some(&share_name));

    // Top Breadcrumb bar card
    draw_card(gfx, th, CONTENT_X, 80, CONTENT_W, 42, false);
    gfx.icon(
        Icon::Folder,
        (CONTENT_X + 26) as f32,
        101.0,
        22.0,
        th.accent_hi,
    );

    let uw = action_width(gfx, PadButton::Circle, s.action_up);
    draw_action_hotspot(
        gfx,
        th,
        hits,
        CONTENT_R - 10 - uw,
        101,
        PadButton::Circle,
        s.action_up,
        UiAction::PickerUp,
        FocusSlot::Header { col: 0 },
        state.is_focused(FocusSlot::Header { col: 0 }),
    );
    let mut right = CONTENT_R - 18 - uw;

    if !state.picker_items.is_empty() {
        let cnt = s.fmt_item_counter(state.selected_picker_idx + 1, state.picker_items.len());
        right -= gfx.text(
            right,
            101,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Right,
            &cnt,
        ) + 14;
    }

    gfx.text_fit(
        CONTENT_X + 50,
        101,
        right - CONTENT_X - 58,
        16,
        FontWeight::Medium,
        th.text,
        Align::Left,
        &state.picker_current_dir,
    );

    let top = state.picker_top_idx;
    let total = state.picker_items.len();

    for i in 0..BROWSER_ROWS {
        let idx = top + i;
        if idx >= total {
            break;
        }
        let y = BROWSER_TOP + (i as i32) * BROWSER_STEP;
        let cy = y + 19;
        let item = &state.picker_items[idx];
        let cur = idx == state.selected_picker_idx;

        if cur {
            gfx.rrect(
                CONTENT_X,
                y,
                CONTENT_W - 14,
                BROWSER_ROW_H,
                10,
                th.accent_bg,
            );
            gfx.rrect(CONTENT_X, y + 8, 4, 22, 2, th.accent);
        }

        let tx = CONTENT_X + 22;
        let ic = if item.is_preset {
            Icon::Plus
        } else {
            Icon::Folder
        };
        // Folders already shared are greyed out and cannot be selected.
        let ic_col = if item.is_shared {
            th.faint
        } else if item.is_preset {
            C_GOOD
        } else {
            th.accent_hi
        };
        gfx.icon(ic, (tx + 12) as f32, cy as f32, 22.0, ic_col);

        let badge = if item.is_shared {
            s.action_shared
        } else if item.is_preset {
            s.hint_select
        } else {
            s.action_open
        };
        // The row chooses the folder; its `Open` chip enters it instead.
        hits.add(Hotspot {
            rect: Rect::new(CONTENT_X, y, CONTENT_W - 14, BROWSER_ROW_H),
            action: UiAction::PickerSelect(idx),
            focus: Some(FocusSlot::RowAction { row: idx, col: 0 }),
            shortcut: None,
        });
        let bw = draw_chip(gfx, CONTENT_R - 26, cy, badge, ic_col);
        if !item.is_shared && !item.is_preset {
            let chip_x = CONTENT_R - 26 - bw - CHIP_TOUCH_MARGIN;
            hits.add(Hotspot {
                rect: Rect::new(chip_x, y, CONTENT_R - 14 - chip_x, BROWSER_ROW_H),
                action: UiAction::PickerOpen(idx),
                focus: None,
                shortcut: None,
            });
        }

        gfx.text_fit(
            tx + 34,
            cy,
            CONTENT_W - 60 - bw,
            16,
            if cur {
                FontWeight::Medium
            } else {
                FontWeight::Regular
            },
            if item.is_shared { th.faint } else { th.text },
            Align::Left,
            &s.text(&item.label),
        );
    }

    draw_scrollbar(
        gfx,
        th,
        CONTENT_R - 6,
        BROWSER_TOP,
        (BROWSER_ROWS as i32) * BROWSER_STEP - 4,
        top,
        BROWSER_ROWS,
        total,
    );

    let selected = state.selected_picker_idx;
    let open_action = (selected < total).then_some(UiAction::PickerOpen(selected));
    draw_hint_bar(
        gfx,
        th,
        hits,
        &[
            Hint::info(PadButton::Dpad, None, s.hint_move),
            Hint::info(
                PadButton::Cross,
                None,
                if state.picker_mode == PickerMode::CreateNewShare {
                    s.hint_share_folder
                } else {
                    s.hint_select_target
                },
            ),
            Hint {
                action: open_action,
                ..Hint::info(PadButton::Triangle, None, s.hint_open_subfolder)
            },
            Hint::action(PadButton::Circle, s.hint_up_back, UiAction::PickerUp),
            Hint::info(PadButton::L, Some(PadButton::R), s.hint_tabs),
        ],
    );
}
