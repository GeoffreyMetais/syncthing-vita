//! Tab 3 (`Files` / `FolderPicker`): Interactive `ux0:` directory browser and folder presets.

use crate::domain::{AppState, PickerMode, BROWSER_ROWS};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::theme::{
    UiTheme, BROWSER_STEP, BROWSER_TOP, CONTENT_R, CONTENT_W, CONTENT_X, C_GOOD,
};
use crate::ui::widgets::{
    action_width, draw_action, draw_card, draw_chip, draw_hints, draw_page_title, draw_scrollbar,
};

/// Renders Tab 3: **Files** (`FolderPicker`).
pub fn render_folder_picker_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let (page_title, share_name) = if state.picker_mode == PickerMode::CreateNewShare {
        (
            "Create Share from Folder",
            "Pick any ux0: folder".to_owned(),
        )
    } else {
        let s = state
            .shares
            .get(state.selected_share_idx)
            .map(|s| format!("For {} ({})", s.label, s.folder_id))
            .unwrap_or_else(|| "Default share".to_owned());
        ("Choose Target Folder", s)
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

    let uw = action_width(gfx, PadButton::Circle, "Up");
    draw_action(gfx, th, CONTENT_R - 10 - uw, 101, PadButton::Circle, "Up");
    let mut right = CONTENT_R - 18 - uw;

    if !state.picker_items.is_empty() {
        let cnt = format!(
            "{} / {}",
            state.selected_picker_idx + 1,
            state.picker_items.len()
        );
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
            gfx.rrect(CONTENT_X, y, CONTENT_W - 14, 38, 10, th.accent_bg);
            gfx.rrect(CONTENT_X, y + 8, 4, 22, 2, th.accent);
        }

        let tx = CONTENT_X + 22;
        let ic = if item.is_preset {
            Icon::Plus
        } else {
            Icon::Folder
        };
        let ic_col = if item.is_preset { C_GOOD } else { th.accent_hi };
        gfx.icon(ic, (tx + 12) as f32, cy as f32, 22.0, ic_col);

        let badge = if item.is_preset { "Select" } else { "Directory" };
        let bw = draw_chip(gfx, CONTENT_R - 26, cy, badge, ic_col);

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
            th.text,
            Align::Left,
            &item.label,
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

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::DpadUd, None, "Move"),
            (
                PadButton::Cross,
                None,
                if state.picker_mode == PickerMode::CreateNewShare {
                    "Create Share Here"
                } else {
                    "Select Target"
                },
            ),
            (PadButton::Triangle, None, "Open Subfolder"),
            (PadButton::Circle, None, "Up / Back"),
            (PadButton::L, Some(PadButton::R), "Tabs"),
        ],
    );
}
