//! Reusable VitaSend-style UI widgets (`draw_card`, `draw_chip`, `draw_action`, `draw_row_tick`,
//! `draw_progress`, `draw_scrollbar`, `draw_page_title`, `draw_chrome`, `draw_hints`, `draw_toast`).

use crate::domain::{ActiveTab, AppState};
use crate::ui::gfx::{rgb, with_alpha, Align, Color, FontWeight, Gfx, Icon, PadButton, GFX_H, GFX_W};
use crate::ui::theme::{
    UiTheme, CONTENT_R, CONTENT_W, CONTENT_X, C_GOOD, C_WARN, HINT_Y, SIDEBAR_W,
};

pub fn draw_card(gfx: &mut Gfx, th: &UiTheme, x: i32, y: i32, w: i32, h: i32, focused: bool) {
    let bg = if focused { th.card_hi } else { th.card };
    gfx.rrect(x, y, w, h, 14, bg);
    if focused {
        gfx.rrect_outline(x, y, w, h, 14, 2, th.accent);
    } else {
        gfx.rrect_outline(x, y, w, h, 14, 1, th.border);
    }
}

pub fn draw_chip(gfx: &mut Gfx, right_x: i32, cy: i32, text: &str, col: Color) -> i32 {
    let w = gfx.text_width(13, FontWeight::Medium, text) + 22;
    gfx.rrect(right_x - w, cy - 12, w, 24, 12, with_alpha(col, 44));
    gfx.text(
        right_x - w / 2,
        cy,
        13,
        FontWeight::Medium,
        col,
        Align::Center,
        text,
    );
    w
}

pub fn action_width(gfx: &mut Gfx, btn: PadButton, label: &str) -> i32 {
    8 + Gfx::pad_button_width(btn, 22) + 8 + gfx.text_width(15, FontWeight::Medium, label) + 16
}

pub fn draw_action(
    gfx: &mut Gfx,
    th: &UiTheme,
    x: i32,
    cy: i32,
    btn: PadButton,
    label: &str,
) -> i32 {
    let gw = Gfx::pad_button_width(btn, 22);
    let tw = gfx.text_width(15, FontWeight::Medium, label);
    let w = 8 + gw + 8 + tw + 16;
    gfx.rrect(x, cy - 17, w, 34, 17, rgb(36, 45, 54));
    gfx.rrect_outline(x, cy - 17, w, 34, 17, 1, th.border);
    gfx.pad_button(btn, x + 8 + gw / 2 + 1, cy, 22);
    gfx.text(
        x + 8 + gw + 8,
        cy,
        15,
        FontWeight::Medium,
        th.text,
        Align::Left,
        label,
    );
    w
}

/// Circular checkbox modeled after VitaSend `row_tick`.
pub fn draw_row_tick(gfx: &mut Gfx, th: &UiTheme, cx: i32, cy: i32, on: bool) {
    if on {
        gfx.circle(cx as f32, cy as f32, 10.0, th.accent);
        gfx.icon(Icon::Check, cx as f32, cy as f32, 16.0, th.on_accent);
    } else {
        gfx.ring(cx as f32, cy as f32, 9.0, 1.8, th.faint);
    }
}

pub fn draw_progress(gfx: &mut Gfx, x: i32, y: i32, w: i32, h: i32, pct: u32, col: Color) {
    gfx.rrect(x, y, w, h, h / 2, rgb(38, 47, 56));
    let p = pct.min(100);
    if p == 0 {
        return;
    }
    let fw = ((w as u32) * p / 100) as i32;
    gfx.rrect(x, y, fw.max(h), h, h / 2, col);
}

pub fn draw_scrollbar(
    gfx: &mut Gfx,
    th: &UiTheme,
    x: i32,
    y: i32,
    h: i32,
    top: usize,
    rows: usize,
    count: usize,
) {
    if count <= rows || rows == 0 {
        return;
    }
    gfx.rrect(x, y, 4, h, 2, rgb(30, 38, 45));
    let thumb_h = ((h as usize) * rows / count).max(16) as i32;
    let thumb_y = y + ((h - thumb_h) as usize * top / (count - rows)) as i32;
    gfx.rrect(x, thumb_y, 4, thumb_h, 2, th.faint);
}

pub fn draw_page_title(gfx: &mut Gfx, th: &UiTheme, title: &str, right: Option<&str>) {
    gfx.text_fit(
        CONTENT_X,
        46,
        CONTENT_W - 200,
        26,
        FontWeight::Bold,
        th.text,
        Align::Left,
        title,
    );
    if let Some(r) = right {
        if !r.is_empty() {
            gfx.text_fit(
                CONTENT_R,
                48,
                240,
                14,
                FontWeight::Regular,
                th.dim,
                Align::Right,
                r,
            );
        }
    }
}

pub fn draw_chrome(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let tabs = [
        ("Overview", Icon::Vita, ActiveTab::Dashboard),
        ("Shares", Icon::Folder, ActiveTab::Shares),
        ("Files", Icon::Receive, ActiveTab::FolderPicker),
        ("Settings", Icon::Settings, ActiveTab::Settings),
    ];

    gfx.clear(th.bg);
    gfx.rect(0, 0, SIDEBAR_W, HINT_Y, th.sidebar);
    gfx.rect(0, HINT_Y, GFX_W, GFX_H - HINT_Y, th.sidebar);
    if th.divider {
        gfx.rect(SIDEBAR_W - 1, 0, 1, HINT_Y, th.border);
        gfx.rect(0, HINT_Y, GFX_W, 1, th.border);
    }

    // Top-left VitaSend-style rounded logo + title
    gfx.rrect(20, 22, 40, 40, 12, th.accent);
    gfx.icon(Icon::Logo, 40.0, 42.0, 34.0, rgb(255, 255, 255));
    gfx.text(
        70,
        42,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        "VitaSync",
    );

    // Sidebar tabs
    for (i, &(name, icon, tab)) in tabs.iter().enumerate() {
        let y = 90 + (i as i32) * 50;
        let on = state.active_tab == tab;
        if on {
            gfx.rrect(12, y, SIDEBAR_W - 24, 44, 12, th.accent_bg);
            gfx.rrect_outline(12, y, SIDEBAR_W - 24, 44, 12, 1, th.accent);
        }
        let fg = if on { th.accent_hi } else { th.dim };
        gfx.icon(icon, 38.0, (y + 22) as f32, 24.0, fg);
        gfx.text(62, y + 22, 17, FontWeight::Medium, fg, Align::Left, name);
    }

    // Sidebar mini transfer card (like VitaSend xfer card)
    let card_y = HINT_Y - 150;
    gfx.rrect(12, card_y, SIDEBAR_W - 24, 82, 12, th.card);
    gfx.rrect_outline(12, card_y, SIDEBAR_W - 24, 82, 12, 1, th.border);
    let xfer_title = if state.sync_in_progress {
        "Syncing BEP/1.0..."
    } else {
        "Last sync session"
    };
    let pct = state.progress_pct.min(100);
    gfx.text(
        24,
        card_y + 20,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        xfer_title,
    );
    gfx.text_fit(
        24,
        card_y + 42,
        SIDEBAR_W - 48,
        14,
        FontWeight::Medium,
        th.text,
        Align::Left,
        &state.peer_label,
    );
    draw_progress(gfx, 24, card_y + 58, SIDEBAR_W - 88, 8, pct, th.accent);
    let pct_str = format!("{}%", pct);
    gfx.text(
        SIDEBAR_W - 24,
        card_y + 62,
        12,
        FontWeight::Medium,
        th.dim,
        Align::Right,
        &pct_str,
    );

    // Sidebar bottom Wi-Fi & version status
    let base = HINT_Y - 18;
    let wifi_ok = !state.peer_label.contains("Scanning");
    gfx.icon(
        Icon::Wifi,
        30.0,
        (base - 4) as f32,
        18.0,
        if wifi_ok { C_GOOD } else { C_WARN },
    );
    let net_txt = if wifi_ok {
        "BEP/1.0 TLS 1.3"
    } else {
        "UDP 21027..."
    };
    gfx.text_fit(
        48,
        base,
        SIDEBAR_W - 60,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        net_txt,
    );
    gfx.text(
        20,
        base - 28,
        12,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        "v0.1.0 · Pure Rust",
    );
}

pub fn draw_hints(gfx: &mut Gfx, th: &UiTheme, items: &[(PadButton, Option<PadButton>, &str)]) {
    gfx.rect(0, HINT_Y, GFX_W, GFX_H - HINT_Y, th.sidebar);
    if th.divider {
        gfx.rect(0, HINT_Y, GFX_W, 1, th.border);
    }
    let cy = HINT_Y + 22;
    let size = 24;
    let mut x = 22;
    for &(b1, b2, label) in items {
        let w1 = Gfx::pad_button_width(b1, size);
        gfx.pad_button(b1, x + w1 / 2, cy, size);
        x += w1 + 4;
        if let Some(sec) = b2 {
            let w2 = Gfx::pad_button_width(sec, size);
            gfx.pad_button(sec, x + w2 / 2, cy, size);
            x += w2 + 4;
        }
        x += 5;
        x += gfx.text(
            x,
            cy,
            15,
            FontWeight::Medium,
            rgb(176, 186, 198),
            Align::Left,
            label,
        ) + 22;
    }
}

pub fn draw_toast(gfx: &mut Gfx, th: &UiTheme, toast_msg: Option<&(String, u32)>) {
    if let Some((msg, _)) = toast_msg {
        let size = 16;
        let maxw = 700;
        let tw = gfx.text_width(size, FontWeight::Medium, msg).min(maxw);
        let w = tw + 48;
        let h = 42;
        let x = (GFX_W - w) / 2;
        let y = HINT_Y - 18 - h;
        gfx.shadow(x, y, w, h, 21);
        gfx.rrect(x, y, w, h, 21, rgb(54, 63, 74));
        gfx.text_fit(
            GFX_W / 2,
            y + h / 2,
            maxw,
            size,
            FontWeight::Medium,
            th.text,
            Align::Center,
            msg,
        );
    }
}
