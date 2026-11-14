//! Full-screen protocol log viewer, opened from the `Settings` log card: this session's lines
//! or, after `Show All`, the whole `syncthing.log` history. It covers the sidebar so log lines
//! get the full screen width.

use crate::domain::{AppState, HitMap, Hotspot, LogScope, LogView, Rect, UiAction, LOG_VIEW_ROWS};
use crate::ui::gfx::{Align, Color, FontWeight, Gfx, PadButton, GFX_W};
use crate::ui::strings;
use crate::ui::theme::{UiTheme, CONTENT_B, C_BAD, C_GOOD};
use crate::ui::widgets::{draw_card, draw_hint_bar, draw_scrollbar, Hint};

/// Left / right margin of the viewer.
const MARGIN: i32 = 24;
/// Top of the log card (below the title).
const CARD_Y: i32 = 74;
/// Distance between two log lines.
const LINE_STEP: i32 = 20;
/// Font size of the log lines.
const LINE_SIZE: i32 = 13;

/// Colour of a log line: errors in red, completed syncs and connections in green.
pub fn log_line_colour(th: &UiTheme, line: &str) -> Color {
    if line.contains("Error") || line.contains("Failed") {
        C_BAD
    } else if line.contains("Synced")
        || line.contains("Connected")
        || line.contains("Sync OK")
        || line.contains("up-to-date")
    {
        C_GOOD
    } else {
        th.dim
    }
}

/// Renders the log viewer `view` over the whole screen.
pub fn render_log_screen(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    view: &LogView,
    hits: &mut HitMap,
) {
    let s = strings::strings(state.language);
    let lines = view.lines(&state.logs);
    let (start, end) = view.window(lines.len());
    let right = GFX_W - MARGIN;

    gfx.clear(th.bg);
    let scope = match view.scope {
        LogScope::Session => s.logs_scope_session,
        LogScope::History => s.logs_scope_history,
    };
    let summary = if lines.is_empty() {
        scope.to_owned()
    } else {
        format!(
            "{scope}  ·  {}",
            s.fmt_log_range(start + 1, end, lines.len())
        )
    };
    let summary_w = gfx.text(
        right,
        46,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Right,
        &summary,
    );
    gfx.text_fit(
        MARGIN,
        44,
        right - MARGIN - summary_w - 24,
        26,
        FontWeight::Bold,
        th.text,
        Align::Left,
        s.logs_title,
    );

    let card_w = right - MARGIN;
    let card_h = CONTENT_B - CARD_Y;
    draw_card(gfx, th, MARGIN, CARD_Y, card_w, card_h, false);
    // Touching the upper half of the list pages towards older lines, the lower half newer.
    hits.add(Hotspot {
        rect: Rect::new(MARGIN, CARD_Y, card_w, card_h / 2),
        action: UiAction::ScrollLogsOlder,
        focus: None,
        shortcut: None,
    });
    hits.add(Hotspot {
        rect: Rect::new(MARGIN, CARD_Y + card_h / 2, card_w, card_h - card_h / 2),
        action: UiAction::ScrollLogsNewer,
        focus: None,
        shortcut: None,
    });

    let rows_h = LOG_VIEW_ROWS as i32 * LINE_STEP;
    let first_cy = CARD_Y + (card_h - rows_h) / 2 + LINE_STEP / 2;
    let text_x = MARGIN + 20;
    let scrollbar_x = right - 14;
    if lines.is_empty() {
        gfx.text(
            MARGIN + card_w / 2,
            CARD_Y + card_h / 2,
            15,
            FontWeight::Regular,
            th.faint,
            Align::Center,
            s.logs_empty,
        );
    }
    for (i, line) in lines[start..end].iter().enumerate() {
        gfx.text_fit(
            text_x,
            first_cy + i as i32 * LINE_STEP,
            scrollbar_x - 12 - text_x,
            LINE_SIZE,
            FontWeight::Regular,
            log_line_colour(th, line),
            Align::Left,
            line,
        );
    }
    draw_scrollbar(
        gfx,
        th,
        scrollbar_x,
        CARD_Y + 12,
        card_h - 24,
        start,
        LOG_VIEW_ROWS,
        lines.len(),
    );

    let toggle = match view.scope {
        LogScope::Session => s.action_show_all,
        LogScope::History => s.action_show_session,
    };
    draw_hint_bar(
        gfx,
        th,
        hits,
        &[
            Hint::info(PadButton::DpadUd, None, s.hint_scroll),
            Hint::action(PadButton::L, s.hint_older, UiAction::ScrollLogsOlder),
            Hint::action(PadButton::R, s.hint_newer, UiAction::ScrollLogsNewer),
            Hint::action(PadButton::Triangle, toggle, UiAction::ToggleLogHistory),
            Hint::action(PadButton::Circle, s.action_back, UiAction::CloseLogs),
        ],
    );
}
