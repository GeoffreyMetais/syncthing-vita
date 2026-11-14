//! Bouncing DVD-style **OLED Screensaver** (`#000000` background) with live transfer progress.

use crate::domain::{AppState, SyncActivity};
use crate::ui::gfx::{rgb, Align, FontWeight, Gfx, Icon};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, C_GOOD};
use crate::ui::widgets::{draw_indeterminate_progress, draw_progress};

pub const SAVER_W: f32 = 410.0;
pub const SAVER_H: f32 = 132.0;

/// Renders the bouncing DVD-style **OLED Screensaver** (`#000000` background).
pub fn render_oled_screensaver(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    let s = strings::strings(state.language);
    gfx.clear(rgb(0, 0, 0));

    let bx = state.saver_x as i32;
    let by = state.saver_y as i32;

    gfx.rrect(bx, by, 64, 64, 18, th.accent);
    gfx.icon(
        Icon::Logo,
        (bx + 32) as f32,
        (by + 32) as f32,
        54.0,
        rgb(255, 255, 255),
    );
    gfx.text(
        bx + 82,
        by + 32,
        38,
        FontWeight::Bold,
        rgb(235, 240, 245),
        Align::Left,
        s.app_title,
    );

    // Same state as the sidebar transfer card (`AppState::sync_activity`). The progress bar is
    // only drawn while a transfer runs: `Some(None)` is an indeterminate bar, `None` no bar.
    let (status_line, bar, colour) = match state.sync_activity() {
        SyncActivity::Downloading => {
            let pct = state.progress_pct.map(|p| p.min(100));
            let line = s.fmt_saver_syncing(pct, &s.text(&state.current_file));
            (line, Some(pct), th.accent_hi)
        }
        SyncActivity::Uploading => {
            let line = s.fmt_saver_uploading(&s.text(&state.current_file));
            (line, Some(None), th.accent_hi)
        }
        SyncActivity::Ready => (s.status_ready.to_owned(), None, C_GOOD),
        SyncActivity::Idle => (s.text(&state.status_line).into_owned(), None, th.accent_hi),
    };
    gfx.text_fit(
        bx + 2,
        by + 88,
        (SAVER_W as i32) - 8,
        16,
        FontWeight::Medium,
        colour,
        Align::Left,
        &status_line,
    );
    match bar {
        Some(Some(p)) => {
            draw_progress(gfx, bx + 2, by + 104, (SAVER_W as i32) - 8, 8, p, th.accent)
        }
        Some(None) => draw_indeterminate_progress(
            gfx,
            bx + 2,
            by + 104,
            (SAVER_W as i32) - 8,
            8,
            state.progress_anim_tick,
            th.accent,
        ),
        None => {}
    }
    gfx.text(
        bx + 2,
        by + 124,
        12,
        FontWeight::Regular,
        rgb(96, 106, 118),
        Align::Left,
        s.saver_wake_hint,
    );
}
