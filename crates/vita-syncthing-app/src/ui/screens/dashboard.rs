//! Tab 1 (`Overview` / `Dashboard`): compact "This device" card (`QR / ID` & `New Share`
//! actions) above the list of shares (`ui/screens/shares.rs`).

use crate::domain::{AppState, FocusSlot, HitMap, Hotspot, Rect, UiAction, UiText};
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton};
use crate::ui::screens::shares::draw_share_list;
use crate::ui::strings;
use crate::ui::theme::{UiTheme, CONTENT_R, CONTENT_W, CONTENT_X, CONTENT_Y, C_GOOD, C_WARN};
use crate::ui::widgets::{
    action_width, draw_action_hotspot, draw_card, draw_chip, draw_hint_bar, draw_page_title, Hint,
};

/// Height of the "This device" card.
const HERO_CARD_H: i32 = 104;
/// Vertical gap between the device card and the share list.
const SECTION_GAP: i32 = 10;

/// Renders Tab 1: **Overview** (`Dashboard`).
pub fn render_dashboard_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let active_count = state.shares.iter().filter(|s| s.enabled).count();
    let subtitle = s.fmt_active_shares_devices(active_count, state.devices.len());
    draw_page_title(gfx, th, s.dashboard_title, Some(&subtitle));

    draw_device_card(gfx, th, state, hits);
    draw_share_list(gfx, th, state, hits, CONTENT_Y + HERO_CARD_H + SECTION_GAP);

    let selected = state.selected_share_idx;
    let has_selection = selected < state.shares.len();
    // `Triangle` is the focused share's `Folder`, and `New Share` anywhere else.
    let triangle = if state.is_share_row_focused() {
        Hint::action(
            PadButton::Triangle,
            s.action_folder,
            UiAction::ChangeShareFolder(selected),
        )
    } else {
        Hint::action(PadButton::Triangle, s.action_new_share, UiAction::NewShare)
    };
    let mut hints = vec![
        Hint::info(PadButton::Dpad, Some(PadButton::Cross), s.hint_navigate),
        Hint::action(PadButton::Circle, s.action_qr_id, UiAction::ShowDeviceId),
        triangle,
    ];
    // `Delete` is only offered when there is a share to delete.
    if has_selection {
        hints.push(Hint::action(
            PadButton::Square,
            s.action_delete,
            UiAction::DeleteShare(selected),
        ));
    }
    hints.push(Hint::info(PadButton::L, Some(PadButton::R), s.hint_tabs));
    hints.push(Hint::action(
        PadButton::Select,
        s.hint_screensaver,
        UiAction::StartScreensaver,
    ));
    draw_hint_bar(gfx, th, hits, &hints);
}

/// "This device" card: name, status, Device ID and the `[QR / ID, New Share]` header buttons.
fn draw_device_card(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let header_focused = state.is_focused(FocusSlot::Header { col: 0 })
        || state.is_focused(FocusSlot::Header { col: 1 });
    draw_card(
        gfx,
        th,
        CONTENT_X,
        CONTENT_Y,
        CONTENT_W,
        HERO_CARD_H,
        header_focused,
    );
    // Touching the device card shows its ID; the buttons declared below sit on top of it.
    hits.add(Hotspot {
        rect: Rect::new(CONTENT_X, CONTENT_Y, CONTENT_W, HERO_CARD_H),
        action: UiAction::ShowDeviceId,
        focus: None,
        shortcut: None,
    });

    let lx = CONTENT_X + 20;
    gfx.rrect(lx, CONTENT_Y + 20, 64, 64, 18, th.accent_bg);
    gfx.icon(
        Icon::Vita,
        (lx + 32) as f32,
        (CONTENT_Y + 52) as f32,
        50.0,
        th.accent_hi,
    );

    let tx = lx + 82;
    let (chip_txt, chip_col) = if state.sync_in_progress {
        (s.status_syncing, th.accent_hi)
    } else if state.status_line == UiText::ScanningLan {
        (s.status_scanning, C_WARN)
    } else {
        (s.status_ready, C_GOOD)
    };
    let cw = draw_chip(gfx, CONTENT_R - 20, CONTENT_Y + 24, chip_txt, chip_col);

    let label_w = gfx.text(
        tx,
        CONTENT_Y + 22,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.hero_this_device,
    );
    let name_x = tx + label_w + 12;
    gfx.text_fit(
        name_x,
        CONTENT_Y + 22,
        CONTENT_R - 32 - cw - name_x,
        17,
        FontWeight::Bold,
        th.text,
        Align::Left,
        s.hero_device_name,
    );

    let act_new_w = action_width(gfx, PadButton::Triangle, s.action_new_share);
    let act_qr_w = action_width(gfx, PadButton::Circle, s.action_qr_id);
    let bx_new = CONTENT_R - 16 - act_new_w;
    let bx_qr = bx_new - 10 - act_qr_w;
    let buttons_cy = CONTENT_Y + 70;
    // Text left of the buttons stays clear of their focus ring.
    let text_w = bx_qr - 16 - tx;

    gfx.text_fit(
        tx,
        CONTENT_Y + 52,
        text_w,
        15,
        FontWeight::Medium,
        th.accent_hi,
        Align::Left,
        &s.text(&state.status_line),
    );

    // The ID value follows the (translated) label instead of a fixed column.
    let id_label_w = gfx.text(
        tx,
        CONTENT_Y + 78,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        s.hero_device_id_label,
    );
    let id_x = tx + id_label_w + 10;
    gfx.text_fit(
        id_x,
        CONTENT_Y + 78,
        bx_qr - 16 - id_x,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &state.vita_device_id,
    );

    let buttons = [
        (
            bx_qr,
            PadButton::Circle,
            s.action_qr_id,
            UiAction::ShowDeviceId,
        ),
        (
            bx_new,
            PadButton::Triangle,
            s.action_new_share,
            UiAction::NewShare,
        ),
    ];
    for (col, (bx, pad, label, action)) in buttons.into_iter().enumerate() {
        let slot = FocusSlot::Header { col };
        draw_action_hotspot(
            gfx,
            th,
            hits,
            bx,
            buttons_cy,
            pad,
            label,
            action,
            slot,
            state.is_focused(slot),
        );
    }
}
