//! Sidebar transfer card: the sync state (`AppState::sync_activity`) and the `Sync now` button.
//! The button stays visible in every state: active and clickable when `Idle` or `Ready`, greyed
//! out and non-clickable while `Downloading` or `Uploading`.

use crate::domain::{AppState, HitMap, Hotspot, Rect, SyncActivity, UiAction};
use crate::ui::gfx::{with_alpha, Align, FontWeight, Gfx, Icon};
use crate::ui::strings::Strings;
use crate::ui::theme::{UiTheme, C_GOOD, HINT_Y, SIDEBAR_W};
use crate::ui::widgets::{draw_indeterminate_progress, draw_progress};

/// Left edge, width and height of the card.
const CARD_X: i32 = 12;
const CARD_W: i32 = SIDEBAR_W - 2 * CARD_X;
const CARD_H: i32 = 90;
/// Left edge and width of the card's content.
const INNER_X: i32 = 24;
const INNER_W: i32 = SIDEBAR_W - 2 * INNER_X;
/// Height of the `Sync now` button.
const CTA_H: i32 = 26;

/// Draws the transfer card above the sidebar's network status and declares it as a touch target
/// (`UiAction::StartSync`) when `Idle` or `Ready` (greyed out and non-clickable while
/// `Downloading` or `Uploading`).
pub fn draw_transfer_card(
    gfx: &mut Gfx,
    th: &UiTheme,
    s: &Strings,
    state: &AppState,
    hits: &mut HitMap,
) {
    let card_y = HINT_Y - 156;
    let activity = state.sync_activity();
    let syncing = matches!(
        activity,
        SyncActivity::Downloading | SyncActivity::Uploading
    );

    gfx.rrect(CARD_X, card_y, CARD_W, CARD_H, 12, th.card);
    if !syncing {
        hits.add(Hotspot {
            rect: Rect::new(CARD_X, card_y, CARD_W, CARD_H),
            action: UiAction::StartSync,
            focus: None,
            shortcut: None,
        });
    }
    gfx.rrect_outline(CARD_X, card_y, CARD_W, CARD_H, 12, 1, th.border);

    draw_sync_button(gfx, th, s, card_y, !syncing);

    match activity {
        SyncActivity::Ready => draw_ready(gfx, s, card_y),
        SyncActivity::Idle => draw_idle(gfx, th, s, state, card_y),
        SyncActivity::Downloading => {
            draw_transfer(
                gfx,
                th,
                s.sidebar_syncing,
                state,
                card_y,
                state.progress_pct,
            );
        }
        SyncActivity::Uploading => {
            // Uploads have no known total: indeterminate bar.
            draw_transfer(gfx, th, s.sidebar_uploading, state, card_y, None);
        }
    }
}

/// Draws the `Sync now` pill button at the top of the card (accented when `enabled`, greyed out
/// while a sync is running).
fn draw_sync_button(gfx: &mut Gfx, th: &UiTheme, s: &Strings, card_y: i32, enabled: bool) {
    let cta_y = card_y + 12;
    let (bg, border, fg) = if enabled {
        (th.accent_bg, with_alpha(th.accent, 160), th.accent_hi)
    } else {
        (with_alpha(th.border, 90), th.border, th.faint)
    };
    gfx.rrect(INNER_X, cta_y, INNER_W, CTA_H, CTA_H / 2, bg);
    gfx.rrect_outline(INNER_X, cta_y, INNER_W, CTA_H, CTA_H / 2, 1, border);

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
        fg,
    );
    gfx.text_fit(
        left + icon_w + gap,
        cy,
        max_label_w,
        14,
        FontWeight::Medium,
        fg,
        Align::Left,
        s.sidebar_sync_cta,
    );
}

/// Connected and up to date: green check and "Ready" below the `Sync now` button.
fn draw_ready(gfx: &mut Gfx, s: &Strings, card_y: i32) {
    let cy = card_y + 62;
    gfx.icon(Icon::Check, (INNER_X + 9) as f32, cy as f32, 18.0, C_GOOD);
    gfx.text_fit(
        INNER_X + 24,
        cy,
        INNER_W - 24,
        15,
        FontWeight::Medium,
        C_GOOD,
        Align::Left,
        s.status_ready,
    );
}

/// No session: peer label below the `Sync now` button.
fn draw_idle(gfx: &mut Gfx, th: &UiTheme, s: &Strings, state: &AppState, card_y: i32) {
    gfx.text_fit(
        INNER_X,
        card_y + 62,
        INNER_W,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        &s.text(&state.peer_label),
    );
}

/// A transfer runs: status title + percentage, then the progress bar (`Some(pct)` determinate,
/// `None` indeterminate).
fn draw_transfer(
    gfx: &mut Gfx,
    th: &UiTheme,
    title: &str,
    state: &AppState,
    card_y: i32,
    pct: Option<u32>,
) {
    let pct_str = match pct {
        Some(p) => {
            let p = p.min(100);
            draw_progress(gfx, INNER_X, card_y + 70, INNER_W, 8, p, th.accent);
            format!("{}%", p)
        }
        None => {
            draw_indeterminate_progress(
                gfx,
                INNER_X,
                card_y + 70,
                INNER_W,
                8,
                state.progress_anim_tick,
                th.accent,
            );
            "...".to_owned()
        }
    };
    gfx.text_fit(
        INNER_X,
        card_y + 53,
        INNER_W - 36,
        12,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        title,
    );
    gfx.text(
        SIDEBAR_W - INNER_X,
        card_y + 53,
        12,
        FontWeight::Medium,
        th.dim,
        Align::Right,
        &pct_str,
    );
}
