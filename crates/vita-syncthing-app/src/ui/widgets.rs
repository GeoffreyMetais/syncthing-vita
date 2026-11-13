//! Reusable VitaSend-style UI widgets (`draw_card`, `draw_chip`, `draw_action`, `draw_row_tick`,
//! `draw_progress`, `draw_scrollbar`, `draw_page_title`, `draw_chrome`, `draw_hints`, `draw_toast`).

use crate::domain::{
    ActiveTab, AppState, FocusSlot, HitMap, Hotspot, NavFocus, PadKey, Rect, ShortcutKey,
    SyncActivity, UiAction, UiText,
};
use crate::ui::gfx::{
    rgb, with_alpha, Align, Color, FontWeight, Gfx, Icon, PadButton, GFX_H, GFX_W,
};
use crate::ui::strings::{self, Strings};
use crate::ui::theme::{
    UiTheme, ACTION_TOUCH_H, CONTENT_R, CONTENT_W, CONTENT_X, C_GOOD, C_WARN, HINT_TOUCH_MARGIN,
    HINT_Y, SIDEBAR_W,
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

/// Draws an action pill like [`draw_action`] and declares it in the frame's `HitMap`: touching
/// it, pressing `Cross` while it holds D-pad focus `focus`, or pressing its face button `btn`
/// (`Cross` excepted) all trigger `action`. The focus ring is drawn when `focused`.
/// Returns the pill width.
#[allow(clippy::too_many_arguments)]
pub fn draw_action_hotspot(
    gfx: &mut Gfx,
    th: &UiTheme,
    hits: &mut HitMap,
    x: i32,
    cy: i32,
    btn: PadButton,
    label: &str,
    action: UiAction,
    focus: FocusSlot,
    focused: bool,
) -> i32 {
    let w = draw_action(gfx, th, x, cy, btn, label);
    if focused {
        draw_focus_ring(gfx, th, x, cy, w);
    }
    let shortcut = shortcut_of(btn);
    hits.add(Hotspot {
        rect: Rect::new(x, cy - ACTION_TOUCH_H / 2, w, ACTION_TOUCH_H),
        action,
        focus: Some(focus),
        shortcut,
    });
    w
}

/// D-pad focus ring around an action pill drawn by `draw_action` at (`x`, `cy`) with width `w`.
pub fn draw_focus_ring(gfx: &mut Gfx, th: &UiTheme, x: i32, cy: i32, w: i32) {
    gfx.rrect_outline(x - 4, cy - 21, w + 8, 42, 21, 2, th.accent_hi);
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

/// Draws an animated indeterminate progress bar for initialization steps before any bytes arrive.
pub fn draw_indeterminate_progress(
    gfx: &mut Gfx,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    tick: u32,
    col: Color,
) {
    gfx.rrect(x, y, w, h, h / 2, rgb(38, 47, 56));
    let seg_w = (w / 3).max(h * 2).min(w);
    let travel = (w - seg_w).max(0);
    let phase = (tick % 120) as i32;
    let pos = if phase < 60 { phase } else { 120 - phase };
    let offset = (travel * pos) / 60;
    gfx.rrect(x + offset, y, seg_w, h, h / 2, col);
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

/// Draws the screen background, the sidebar (tabs, transfer card, status) and declares the
/// sidebar's interactive elements.
pub fn draw_chrome(gfx: &mut Gfx, th: &UiTheme, state: &AppState, hits: &mut HitMap) {
    let s = strings::strings(state.language);
    let tabs = [
        (s.tab_overview, Icon::Vita, ActiveTab::Dashboard),
        (s.tab_files, Icon::Receive, ActiveTab::FolderPicker),
        (s.tab_settings, Icon::Settings, ActiveTab::Settings),
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
        s.app_title,
    );

    // Sidebar tabs
    for (i, &(name, icon, tab)) in tabs.iter().enumerate() {
        let y = 90 + (i as i32) * 50;
        let on = state.active_tab == tab;
        hits.add(Hotspot {
            rect: Rect::new(0, y - 3, SIDEBAR_W, 50),
            action: UiAction::SwitchTab(tab),
            focus: None,
            shortcut: None,
        });
        if on {
            gfx.rrect(12, y, SIDEBAR_W - 24, 44, 12, th.accent_bg);
            // A thicker, brighter outline shows the sidebar has D-pad focus
            let (width, col) = if state.nav.focus == NavFocus::Sidebar {
                (2, th.accent_hi)
            } else {
                (1, th.accent)
            };
            gfx.rrect_outline(12, y, SIDEBAR_W - 24, 44, 12, width, col);
        }
        let fg = if on { th.accent_hi } else { th.dim };
        gfx.icon(icon, 38.0, (y + 22) as f32, 24.0, fg);
        gfx.text(62, y + 22, 17, FontWeight::Medium, fg, Align::Left, name);
    }

    // Sidebar mini transfer card (like VitaSend xfer card)
    let card_y = HINT_Y - 150;
    gfx.rrect(12, card_y, SIDEBAR_W - 24, 82, 12, th.card);
    // Touching the transfer card starts a sync.
    hits.add(Hotspot {
        rect: Rect::new(12, card_y, SIDEBAR_W - 24, 82),
        action: UiAction::StartSync,
        focus: None,
        shortcut: None,
    });
    gfx.rrect_outline(12, card_y, SIDEBAR_W - 24, 82, 12, 1, th.border);
    let activity = state.sync_activity();
    if activity == SyncActivity::Ready {
        // Connected and up to date: just a green check and "Ready".
        gfx.icon(Icon::Check, 44.0, (card_y + 41) as f32, 26.0, C_GOOD);
        gfx.text_fit(
            66,
            card_y + 41,
            SIDEBAR_W - 90,
            18,
            FontWeight::Medium,
            C_GOOD,
            Align::Left,
            s.status_ready,
        );
    } else {
        let xfer_title = match activity {
            SyncActivity::Downloading => s.sidebar_syncing,
            SyncActivity::Uploading => s.sidebar_uploading,
            SyncActivity::Ready | SyncActivity::Idle => s.sidebar_last_sync,
        };
        // Uploads have no known total: an indeterminate bar instead of a stale percentage.
        let progress = if activity == SyncActivity::Uploading {
            None
        } else {
            state.progress_pct
        };
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
            &s.text(&state.peer_label),
        );
        let pct_str = match progress {
            Some(p) => {
                let p = p.min(100);
                draw_progress(gfx, 24, card_y + 58, SIDEBAR_W - 88, 8, p, th.accent);
                format!("{}%", p)
            }
            None => {
                draw_indeterminate_progress(
                    gfx,
                    24,
                    card_y + 58,
                    SIDEBAR_W - 88,
                    8,
                    state.progress_anim_tick,
                    th.accent,
                );
                "...".to_owned()
            }
        };
        gfx.text(
            SIDEBAR_W - 24,
            card_y + 62,
            12,
            FontWeight::Medium,
            th.dim,
            Align::Right,
            &pct_str,
        );
    }

    // Sidebar bottom Wi-Fi & version status
    let base = HINT_Y - 18;
    let wifi_ok = state.status_line != UiText::ScanningLan;
    gfx.icon(
        Icon::Wifi,
        30.0,
        (base - 4) as f32,
        18.0,
        if wifi_ok { C_GOOD } else { C_WARN },
    );
    let net_txt = if wifi_ok {
        s.protocol_tls
    } else {
        s.protocol_discovery
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
        s.app_version_label,
    );
}

/// One entry of the bottom hint bar: the button(s) it shows, what they do, and optionally the
/// action triggered by that button and by touching the hint.
#[derive(Debug, Clone, Copy)]
pub struct Hint<'a> {
    pub button: PadButton,
    pub second: Option<PadButton>,
    pub label: &'a str,
    pub action: Option<UiAction>,
}

impl<'a> Hint<'a> {
    /// A purely informative hint (navigation, tabs, ...).
    pub const fn info(button: PadButton, second: Option<PadButton>, label: &'a str) -> Self {
        Self {
            button,
            second,
            label,
            action: None,
        }
    }

    /// A hint whose button triggers `action`; touching the hint does the same.
    pub const fn action(button: PadButton, label: &'a str, action: UiAction) -> Self {
        Self {
            button,
            second: None,
            label,
            action: Some(action),
        }
    }
}

/// Face button an element displaying `button` declares as its shortcut (`Cross` is reserved
/// for the focused element, the other buttons are handled globally).
fn shortcut_of(button: PadButton) -> Option<ShortcutKey> {
    match button {
        PadButton::Circle => Some(ShortcutKey::Circle),
        PadButton::Square => Some(ShortcutKey::Square),
        PadButton::Triangle => Some(ShortcutKey::Triangle),
        _ => None,
    }
}

/// Width of a hint: its button glyph(s) and its label.
fn hint_width(gfx: &mut Gfx, hint: &Hint, size: i32) -> i32 {
    let second = hint
        .second
        .map_or(0, |b| Gfx::pad_button_width(b, size) + 4);
    Gfx::pad_button_width(hint.button, size)
        + 4
        + second
        + 5
        + gfx.text_width(15, FontWeight::Medium, hint.label)
}

/// Draws the bottom hint bar and declares its actionable hints in the frame's `HitMap`:
/// touching a hint, or pressing the face button it shows, triggers its action.
pub fn draw_hint_bar(gfx: &mut Gfx, th: &UiTheme, hits: &mut HitMap, items: &[Hint]) {
    gfx.rect(0, HINT_Y, GFX_W, GFX_H - HINT_Y, th.sidebar);
    if th.divider {
        gfx.rect(0, HINT_Y, GFX_W, 1, th.border);
    }
    let cy = HINT_Y + 22;
    let size = 24;
    let mut x = 22;
    for hint in items {
        let start_x = x;
        // A hint that does not fit on the bar is left out instead of being cut off.
        if start_x + hint_width(gfx, hint, size) > GFX_W - HINT_TOUCH_MARGIN {
            break;
        }
        let w1 = Gfx::pad_button_width(hint.button, size);
        gfx.pad_button(hint.button, x + w1 / 2, cy, size);
        x += w1 + 4;
        if let Some(sec) = hint.second {
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
            hint.label,
        );
        if let Some(action) = hint.action {
            hits.add(Hotspot {
                rect: Rect::new(
                    start_x - HINT_TOUCH_MARGIN,
                    HINT_Y,
                    x - start_x + 2 * HINT_TOUCH_MARGIN,
                    GFX_H - HINT_Y,
                ),
                action,
                focus: None,
                shortcut: shortcut_of(hint.button),
            });
        }
        x += 22;
    }
}

/// Button a touch element showing `button` stands for (none for the D-pad glyphs).
fn pad_key_of(button: PadButton) -> Option<PadKey> {
    match button {
        PadButton::Cross => Some(PadKey::Cross),
        PadButton::Circle => Some(PadKey::Circle),
        PadButton::Square => Some(PadKey::Square),
        PadButton::Triangle => Some(PadKey::Triangle),
        PadButton::L => Some(PadKey::L),
        PadButton::R => Some(PadKey::R),
        PadButton::Start => Some(PadKey::Start),
        PadButton::Select => Some(PadKey::Select),
        PadButton::Dpad | PadButton::DpadLr | PadButton::DpadUd => None,
    }
}

/// Declares a touch-only element of a modal dialog covering (`x`, `y`, `w`, `h`).
pub fn declare_touch(hits: &mut HitMap, x: i32, y: i32, w: i32, h: i32, action: UiAction) {
    hits.add(Hotspot {
        rect: Rect::new(x, y, w, h),
        action,
        focus: None,
        shortcut: None,
    });
}

/// Declares what touching a modal dialog does outside its buttons: `outside` its card
/// (`x`, `y`, `w`, `h`) and `inside` it. `None` absorbs the touch. Call it right after drawing
/// the card, before the modal's own elements.
pub fn declare_modal(
    hits: &mut HitMap,
    (x, y, w, h): (i32, i32, i32, i32),
    outside: Option<PadKey>,
    inside: Option<PadKey>,
) {
    let action = |key: Option<PadKey>| key.map_or(UiAction::Inert, UiAction::Press);
    declare_touch(hits, 0, 0, GFX_W, GFX_H, action(outside));
    declare_touch(hits, x, y, w, h, action(inside));
}

/// Draws an action pill of a modal dialog like [`draw_action`]; touching it is the same as
/// pressing the button it shows. Returns the pill width.
pub fn draw_press_button(
    gfx: &mut Gfx,
    th: &UiTheme,
    hits: &mut HitMap,
    x: i32,
    cy: i32,
    btn: PadButton,
    label: &str,
) -> i32 {
    let w = draw_action(gfx, th, x, cy, btn, label);
    if let Some(key) = pad_key_of(btn) {
        declare_touch(
            hits,
            x,
            cy - ACTION_TOUCH_H / 2,
            w,
            ACTION_TOUCH_H,
            UiAction::Press(key),
        );
    }
    w
}

/// Draws the hint bar of a modal dialog: touching a hint is the same as pressing the (single)
/// button it shows.
pub fn draw_press_hints(
    gfx: &mut Gfx,
    th: &UiTheme,
    hits: &mut HitMap,
    items: &[(PadButton, Option<PadButton>, &str)],
) {
    let hints: Vec<Hint> = items
        .iter()
        .map(|&(button, second, label)| Hint {
            button,
            second,
            label,
            action: match second {
                None => pad_key_of(button).map(UiAction::Press),
                Some(_) => None,
            },
        })
        .collect();
    draw_hint_bar(gfx, th, hits, &hints);
}

pub fn draw_toast(gfx: &mut Gfx, th: &UiTheme, s: &Strings, toast_msg: Option<&(UiText, u32)>) {
    if let Some((msg, _)) = toast_msg {
        let msg = s.text(msg);
        let msg = msg.as_ref();
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
