//! Tab 4 (`Settings & Log` / `Settings`): Dark / Pure-Black OLED theme switcher,
//! `QR / ID` modal shortcut, UI language (`Change Language`), declined folder offers (`Clear Declined`), and BEP/1.0 & TLS 1.3
//! protocol event log.

use crate::domain::{
    AppState, FocusSlot, HitMap, Hotspot, LanguagePreference, Rect, ShortcutKey, ThemeMode,
    UiAction, SETTINGS_ROW_DECLINED, SETTINGS_ROW_DEVICES, SETTINGS_ROW_DISPLAY,
    SETTINGS_ROW_LANGUAGE,
};
use crate::ui::gfx::{Align, FontWeight, Gfx, PadButton};
use crate::ui::strings;
use crate::ui::theme::{
    UiTheme, CONTENT_B, CONTENT_R, CONTENT_W, CONTENT_X, CONTENT_Y, C_BAD, C_GOOD,
    SETTINGS_DECLINED_B, SETTINGS_DECLINED_Y, SETTINGS_DEVICES_B, SETTINGS_DEVICES_Y,
    SETTINGS_LANGUAGE_B, SETTINGS_LANGUAGE_Y,
};
use crate::ui::widgets::{
    action_width, draw_action_hotspot, draw_card, draw_hint_bar, draw_page_title, Hint,
};

/// Height of the display & theme card.
const DISPLAY_CARD_H: i32 = 110;

/// Vertical centre of the theme card action: beside the short "Colour Theme"
/// label, between the section header and the full-width description line.
const THEME_ACTIONS_Y: i32 = CONTENT_Y + 42;

/// Renders Tab 4: **Settings & Log** (`Settings`).
pub fn render_settings_tab(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    draw_page_title(gfx, th, s.settings_title, Some(s.settings_subtitle));

    // 1. Display & Theme Settings Card
    draw_card(gfx, th, CONTENT_X, CONTENT_Y, CONTENT_W, DISPLAY_CARD_H, true);
    // Touching the card (outside its button) or pressing `Square` switches the theme.
    hits.add(Hotspot {
        rect: Rect::new(CONTENT_X, CONTENT_Y, CONTENT_W, DISPLAY_CARD_H),
        action: UiAction::ToggleTheme,
        focus: None,
        shortcut: Some(ShortcutKey::Square),
    });
    let lx = CONTENT_X + 24;
    gfx.text(
        lx,
        CONTENT_Y + 18,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.settings_sec_display,
    );

    let theme_desc = match state.theme_mode {
        ThemeMode::Dark => s.settings_desc_theme_dark,
        ThemeMode::Oled => s.settings_desc_theme_oled,
    };
    gfx.text(
        lx,
        CONTENT_Y + 46,
        16,
        FontWeight::Medium,
        th.text,
        Align::Left,
        s.settings_label_colour_theme,
    );
    let w_theme = action_width(gfx, PadButton::Cross, s.action_switch_theme);
    let bx_theme = CONTENT_R - 20 - w_theme;

    // The action ends above this line, so the description gets the full card width.
    gfx.text_fit(
        lx,
        CONTENT_Y + 72,
        CONTENT_R - 24 - lx,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        theme_desc,
    );
    let display_slot = FocusSlot::RowAction {
        row: SETTINGS_ROW_DISPLAY,
        col: 0,
    };
    draw_action_hotspot(
        gfx,
        th,
        hits,
        bx_theme,
        THEME_ACTIONS_Y,
        PadButton::Cross,
        s.action_switch_theme,
        UiAction::ToggleTheme,
        display_slot,
        state.is_focused(display_slot),
    );

    gfx.text(
        lx,
        CONTENT_Y + 94,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        s.settings_saver_auto_hint,
    );

    // 2. Devices & Pairing Card
    render_devices(gfx, th, state, hits);

    // 3. UI Language Card
    render_language(gfx, th, state, hits);

    // 4. Declined Folder Offers Card
    render_declined_folders(gfx, th, state, hits);

    // 5. Full Protocol Event Log Card
    let ly = SETTINGS_DECLINED_B + 8;
    let lh = CONTENT_B - ly;
    draw_card(gfx, th, CONTENT_X, ly, CONTENT_W, lh, false);
    gfx.text(
        lx,
        ly + 22,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.settings_sec_log,
    );

    let max_lines = ((lh - 56) / 22).max(0) as usize;
    for (i, line) in state.logs.iter().rev().take(max_lines).enumerate() {
        let y = ly + 48 + (i as i32) * 22;
        let col = if line.contains("Error") || line.contains("Failed") {
            C_BAD
        } else if line.contains("Synced")
            || line.contains("Connected")
            || line.contains("Sync OK")
            || line.contains("up-to-date")
        {
            C_GOOD
        } else {
            th.dim
        };
        gfx.text_fit(
            lx,
            y,
            CONTENT_W - 48,
            13,
            FontWeight::Regular,
            col,
            Align::Left,
            line,
        );
    }

    draw_hint_bar(
        gfx,
        th,
        hits,
        &[
            Hint::info(PadButton::Dpad, Some(PadButton::Cross), s.hint_navigate),
            Hint::action(PadButton::Circle, s.action_qr_id, UiAction::ShowDeviceId),
            Hint::action(PadButton::Triangle, s.action_add_device_id, UiAction::AddDevice),
            Hint::action(PadButton::Select, s.hint_screensaver, UiAction::StartScreensaver),
            Hint::action(PadButton::Start, s.action_quit, UiAction::ConfirmQuit),
        ],
    );
}

/// Dedicated card for device pairing (`Add device`) and viewing this PS Vita's `QR / ID`.
fn render_devices(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let y = SETTINGS_DEVICES_Y;
    let h = SETTINGS_DEVICES_B - SETTINGS_DEVICES_Y;
    draw_card(gfx, th, CONTENT_X, y, CONTENT_W, h, false);
    let lx = CONTENT_X + 24;
    gfx.text(
        lx,
        y + 18,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.settings_sec_devices,
    );

    let w_qr = action_width(gfx, PadButton::Cross, s.action_qr_id);
    let w_add = action_width(gfx, PadButton::Triangle, s.action_add_device);
    let bx_add = CONTENT_R - 20 - w_add;
    let bx_qr = bx_add - 8 - w_qr;
    let cy = y + h / 2;

    gfx.text_fit(
        lx,
        y + 37,
        bx_qr - lx - 16,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        s.settings_desc_devices,
    );

    let buttons = [
        (bx_qr, PadButton::Cross, s.action_qr_id, UiAction::ShowDeviceId),
        (
            bx_add,
            PadButton::Triangle,
            s.action_add_device,
            UiAction::AddDevice,
        ),
    ];
    for (col, (bx, pad, label, action)) in buttons.into_iter().enumerate() {
        let slot = FocusSlot::RowAction {
            row: SETTINGS_ROW_DEVICES,
            col,
        };
        draw_action_hotspot(
            gfx,
            th,
            hits,
            bx,
            cy,
            pad,
            label,
            action,
            slot,
            state.is_focused(slot),
        );
    }
}

/// Card showing the UI language (system default or a fixed one) with a `Change Language` button.
fn render_language(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let y = SETTINGS_LANGUAGE_Y;
    let h = SETTINGS_LANGUAGE_B - SETTINGS_LANGUAGE_Y;
    draw_card(gfx, th, CONTENT_X, y, CONTENT_W, h, false);
    // Touching the card anywhere cycles the language.
    hits.add(Hotspot {
        rect: Rect::new(CONTENT_X, y, CONTENT_W, h),
        action: UiAction::CycleLanguage,
        focus: None,
        shortcut: None,
    });
    let lx = CONTENT_X + 24;
    gfx.text(
        lx,
        y + 18,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.settings_sec_language,
    );
    let value = match state.language_pref {
        LanguagePreference::System => {
            format!("{} · {}", s.settings_language_system, state.language.name())
        }
        LanguagePreference::Fixed(lang) => lang.name().to_owned(),
    };
    let label = s.action_change_language;
    let w_btn = action_width(gfx, PadButton::Cross, label);
    let bx = CONTENT_R - 20 - w_btn;
    let cy = y + h / 2;
    gfx.text_fit(
        lx,
        y + 37,
        bx - lx - 16,
        14,
        FontWeight::Medium,
        th.text,
        Align::Left,
        &value,
    );
    let slot = FocusSlot::RowAction {
        row: SETTINGS_ROW_LANGUAGE,
        col: 0,
    };
    draw_action_hotspot(
        gfx,
        th,
        hits,
        bx,
        cy,
        PadButton::Cross,
        label,
        UiAction::CycleLanguage,
        slot,
        state.is_focused(slot),
    );
}

/// Card listing folder offers declined in the `FolderOffer` modal, with a `Clear Declined` button.
fn render_declined_folders(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let y = SETTINGS_DECLINED_Y;
    let h = SETTINGS_DECLINED_B - SETTINGS_DECLINED_Y;
    draw_card(gfx, th, CONTENT_X, y, CONTENT_W, h, false);
    let lx = CONTENT_X + 24;
    gfx.text(
        lx,
        y + 20,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        s.settings_sec_declined,
    );
    let summary = if state.ignored_folders.is_empty() {
        s.settings_no_declined.to_owned()
    } else {
        let ids: Vec<&str> = state
            .ignored_folders
            .iter()
            .map(|f| f.folder_id.as_str())
            .collect();
        s.fmt_declined_summary(ids.len(), &ids.join(", "))
    };
    let label = s.action_clear_declined;
    let w_clear = action_width(gfx, PadButton::Cross, label);
    let bx = CONTENT_R - 20 - w_clear;
    let cy = y + h / 2;
    let col = if state.ignored_folders.is_empty() {
        th.faint
    } else {
        th.dim
    };
    gfx.text_fit(
        lx,
        y + 44,
        bx - lx - 16,
        14,
        FontWeight::Regular,
        col,
        Align::Left,
        &summary,
    );
    let slot = FocusSlot::RowAction {
        row: SETTINGS_ROW_DECLINED,
        col: 0,
    };
    draw_action_hotspot(
        gfx,
        th,
        hits,
        bx,
        cy,
        PadButton::Cross,
        label,
        UiAction::ClearDeclinedFolders,
        slot,
        state.is_focused(slot),
    );
}
