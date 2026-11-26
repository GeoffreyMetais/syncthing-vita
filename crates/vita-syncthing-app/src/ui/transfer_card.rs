//! Sidebar transfer card: the sync state (`AppState::sync_activity`) and the `Sync now` button.
//! The button is shown and clickable when `Idle` or `Ready`; during `Downloading` or `Uploading`
//! it is replaced by the transfer status and progress bar, and the card is non-clickable.

use crate::domain::{AppState, HitMap, Hotspot, NavFocus, Rect, SyncActivity, UiAction};
use crate::ui::gfx::{with_alpha, Align, FontWeight, Gfx, Icon};
use crate::ui::i18n::Strings;
use crate::ui::theme::{UiTheme, HINT_Y, SIDEBAR_W};
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
/// (`UiAction::StartSync`) when `Idle` or `Ready` (non-clickable while `Downloading` or
/// `Uploading`).
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

    let card_r = th.card_radius.min(12);
    gfx.rrect(CARD_X, card_y, CARD_W, CARD_H, card_r, th.card);
    if !syncing {
        hits.add(Hotspot {
            rect: Rect::new(CARD_X, card_y, CARD_W, CARD_H),
            action: UiAction::StartSync,
            focus: None,
            shortcut: None,
        });
    }
    gfx.rrect_outline(CARD_X, card_y, CARD_W, CARD_H, card_r, 1, th.border);

    let focused = state.nav.focus == NavFocus::SyncCard;
    match activity {
        SyncActivity::Ready => {
            draw_sync_button(gfx, th, s, card_y, focused);
            draw_ready(gfx, th, s, card_y);
        }
        SyncActivity::Idle => {
            draw_sync_button(gfx, th, s, card_y, focused);
            draw_idle(gfx, th, s, state, card_y);
        }
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

/// Draws the `Sync now` button at the top of the card.
fn draw_sync_button(gfx: &mut Gfx, th: &UiTheme, s: &Strings, card_y: i32, focused: bool) {
    let cta_y = card_y + 12;
    let btn_r = th.button_radius.min(CTA_H / 2);
    gfx.rrect(INNER_X, cta_y, INNER_W, CTA_H, btn_r, th.accent_bg);
    gfx.rrect_outline(
        INNER_X,
        cta_y,
        INNER_W,
        CTA_H,
        btn_r,
        1,
        with_alpha(th.accent, 160),
    );
    if focused {
        let focus_r = (btn_r + 3).min((CTA_H + 6) / 2);
        gfx.rrect_outline(
            INNER_X - 3,
            cta_y - 3,
            INNER_W + 6,
            CTA_H + 6,
            focus_r,
            2,
            th.accent_hi,
        );
    }

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
}

/// Connected and up to date: green check and "Ready" below the `Sync now` button.
fn draw_ready(gfx: &mut Gfx, th: &UiTheme, s: &Strings, card_y: i32) {
    let cy = card_y + 62;
    gfx.icon(Icon::Check, (INNER_X + 9) as f32, cy as f32, 18.0, th.good);
    gfx.text_fit(
        INNER_X + 24,
        cy,
        INNER_W - 24,
        15,
        FontWeight::Medium,
        th.good,
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
        card_y + 22,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        title,
    );
    gfx.text_fit(
        INNER_X,
        card_y + 44,
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
            draw_progress(
                gfx,
                th,
                INNER_X,
                card_y + 64,
                SIDEBAR_W - 88,
                8,
                p,
                th.accent,
            );
            format!("{}%", p)
        }
        None => {
            draw_indeterminate_progress(
                gfx,
                th,
                INNER_X,
                card_y + 64,
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
        card_y + 68,
        12,
        FontWeight::Medium,
        th.dim,
        Align::Right,
        &pct_str,
    );
}
