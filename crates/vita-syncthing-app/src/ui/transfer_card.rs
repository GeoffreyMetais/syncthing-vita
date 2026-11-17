//! Sidebar transfer card: the sync state (`AppState::sync_activity`) and the touch target that
//! starts a sync. While idle it shows a `Sync now` button, so that touching the card is visibly
//! the way to connect to every paired device.

use crate::domain::{AppState, HitMap, Hotspot, Rect, SyncActivity, UiAction};
use crate::ui::gfx::{with_alpha, Align, FontWeight, Gfx, Icon};
use crate::ui::strings::Strings;
use crate::ui::theme::{UiTheme, C_GOOD, HINT_Y, SIDEBAR_W};
use crate::ui::widgets::{draw_indeterminate_progress, draw_progress};

/// Left edge, width and height of the card.
const CARD_X: i32 = 12;
const CARD_W: i32 = SIDEBAR_W - 2 * CARD_X;
const CARD_H: i32 = 82;
/// Left edge and width of the card's content.
const INNER_X: i32 = 24;
const INNER_W: i32 = SIDEBAR_W - 2 * INNER_X;
/// Height of the idle `Sync now` button.
const CTA_H: i32 = 26;

/// Draws the transfer card above the sidebar's network status and declares it as a touch target
/// (`UiAction::StartSync`) when `Idle` or `Ready` (non-clickable while `Downloading` or `Uploading`).
pub fn draw_transfer_card(
    gfx: &mut Gfx,
    th: &UiTheme,
    s: &Strings,
    state: &AppState,
    hits: &mut HitMap,
) {
    let card_y = HINT_Y - 150;
    let activity = state.sync_activity();
    gfx.rrect(CARD_X, card_y, CARD_W, CARD_H, 12, th.card);
    if matches!(activity, SyncActivity::Idle | SyncActivity::Ready) {
        hits.add(Hotspot {
            rect: Rect::new(CARD_X, card_y, CARD_W, CARD_H),
            action: UiAction::StartSync,
            focus: None,
            shortcut: None,
        });
    }
    gfx.rrect_outline(CARD_X, card_y, CARD_W, CARD_H, 12, 1, th.border);
    match activity {
        SyncActivity::Ready => draw_ready(gfx, s, card_y),
        SyncActivity::Idle => draw_idle(gfx, th, s, state, card_y),
        SyncActivity::Downloading => {
            draw_transfer(gfx, th, s.sidebar_syncing, s, state, card_y);
            draw_bar(gfx, th, state, card_y, state.progress_pct);
        }
        SyncActivity::Uploading => {
            draw_transfer(gfx, th, s.sidebar_uploading, s, state, card_y);
            // Uploads have no known total: indeterminate bar.
            draw_bar(gfx, th, state, card_y, None);
        }
    }
}

/// Connected and up to date: just a green check and "Ready".
fn draw_ready(gfx: &mut Gfx, s: &Strings, card_y: i32) {
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
}

/// No session: a `Sync now` button (the whole card is the touch target), then the peer label.
fn draw_idle(gfx: &mut Gfx, th: &UiTheme, s: &Strings, state: &AppState, card_y: i32) {
    let cta_y = card_y + 13;
    gfx.rrect(INNER_X, cta_y, INNER_W, CTA_H, CTA_H / 2, th.accent_bg);
    gfx.rrect_outline(
        INNER_X,
        cta_y,
        INNER_W,
        CTA_H,
        CTA_H / 2,
        1,
        with_alpha(th.accent, 160),
    );
    // Refresh icon and label, centred together in the button.
    let icon_w = 16;
    let gap = 6;
    let max_label_w = INNER_W - 24 - icon_w - gap;
    let label_w = gfx
        .text_width(14, FontWeight::Medium, s.sidebar_sync_cta)
        .min(max_label_w);
    let left = INNER_X + (INNER_W - icon_w - gap - label_w) / 2;
    let cy = cta_y + CTA_H / 2;
    gfx.icon(
        Icon::Refresh,
        (left + icon_w / 2) as f32,
        cy as f32,
        icon_w as f32,
        th.accent_hi,
    );
    gfx.text_fit(
        left + icon_w + gap,
        cy,
        max_label_w,
        14,
        FontWeight::Medium,
        th.accent_hi,
        Align::Left,
        s.sidebar_sync_cta,
    );
    gfx.text_fit(
        INNER_X,
        card_y + 54,
        INNER_W,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &s.text(&state.peer_label),
    );
}

/// A transfer runs: its title, then the peer label.
fn draw_transfer(
    gfx: &mut Gfx,
    th: &UiTheme,
    title: &str,
    s: &Strings,
    state: &AppState,
    card_y: i32,
) {
    gfx.text(
        INNER_X,
        card_y + 20,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        title,
    );
    gfx.text_fit(
        INNER_X,
        card_y + 42,
        INNER_W,
        14,
        FontWeight::Medium,
        th.text,
        Align::Left,
        &s.text(&state.peer_label),
    );
}

/// Progress bar and its percentage: `Some(pct)` determinate, `None` indeterminate.
fn draw_bar(gfx: &mut Gfx, th: &UiTheme, state: &AppState, card_y: i32, pct: Option<u32>) {
    let pct_str = match pct {
        Some(p) => {
            let p = p.min(100);
            draw_progress(gfx, INNER_X, card_y + 58, SIDEBAR_W - 88, 8, p, th.accent);
            format!("{}%", p)
        }
        None => {
            draw_indeterminate_progress(
                gfx,
                INNER_X,
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
        SIDEBAR_W - INNER_X,
        card_y + 62,
        12,
        FontWeight::Medium,
        th.dim,
        Align::Right,
        &pct_str,
    );
}
