//! `LogView` window arithmetic, scope switching, history cap and line sanitizing (domain level).
//! The viewer on screen is covered by `log_viewer.rs`.

use vita_syncthing_app::domain::defaults::LOG_HISTORY_MAX_LINES;
use vita_syncthing_app::domain::{
    sanitize_log_line, LogScope, LogView, LOG_VIEW_PAGE, LOG_VIEW_ROWS,
};

fn lines(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("line {i}")).collect()
}

fn view_at(from_end: usize) -> LogView {
    LogView {
        from_end,
        ..LogView::new()
    }
}

#[test]
fn test_rows_and_page_constants() {
    assert_eq!(LOG_VIEW_ROWS, 19);
    assert_eq!(LOG_VIEW_PAGE, 18, "a page keeps one line of context");
}

#[test]
fn test_new_and_default_show_session_newest_lines() {
    let fresh = LogView::new();
    assert_eq!(fresh.scope, LogScope::Session);
    assert_eq!(fresh.from_end, 0);
    assert!(fresh.history.is_empty());
    assert_eq!(LogView::default(), fresh);
}

#[test]
fn test_lines_selects_session_or_history() {
    let session = lines(3);
    let mut view = LogView::new();
    assert_eq!(view.lines(&session), session.as_slice());

    view.show_history(lines(2));
    assert_eq!(view.scope, LogScope::History);
    assert_eq!(
        view.lines(&session),
        &["line 0".to_owned(), "line 1".to_owned()]
    );
}

#[test]
fn test_window_is_empty_without_lines() {
    assert_eq!(view_at(0).window(0), (0, 0));
    assert_eq!(view_at(5).window(0), (0, 0));
}

#[test]
fn test_window_shows_at_most_rows_lines_anchored_on_the_newest() {
    // Fewer lines than rows: everything is shown.
    assert_eq!(view_at(0).window(10), (0, 10));
    assert_eq!(view_at(0).window(LOG_VIEW_ROWS), (0, LOG_VIEW_ROWS));
    // One line more than the window: the oldest is scrolled out.
    assert_eq!(view_at(0).window(LOG_VIEW_ROWS + 1), (1, LOG_VIEW_ROWS + 1));
    // 30 lines: the newest window is 11..30.
    assert_eq!(view_at(0).window(30), (11, 30));
}

#[test]
fn test_window_moves_back_with_from_end() {
    assert_eq!(view_at(3).window(30), (8, 27));
    assert_eq!(view_at(11).window(30), (0, 19));
    // Beyond the oldest line the window stays on the first lines.
    assert_eq!(view_at(100).window(30), (0, 19));
}

#[test]
fn test_scroll_older_moves_one_step_and_stops_at_the_oldest_line() {
    let mut view = LogView::new();
    view.scroll_older(1, 30);
    assert_eq!(view.from_end, 1);
    view.scroll_older(LOG_VIEW_PAGE, 30);
    assert_eq!(view.from_end, 11, "capped at total - rows");
    assert_eq!(view.window(30), (0, 19));
    view.scroll_older(5, 30);
    assert_eq!(view.from_end, 11);
}

#[test]
fn test_scroll_older_by_zero_changes_nothing() {
    let mut view = view_at(4);
    view.scroll_older(0, 30);
    assert_eq!(view.from_end, 4);
}

#[test]
fn test_scroll_newer_moves_back_towards_the_end_and_stops_at_zero() {
    let mut view = view_at(11);
    view.scroll_newer(4, 30);
    assert_eq!(view.from_end, 7);
    view.scroll_newer(LOG_VIEW_PAGE, 30);
    assert_eq!(view.from_end, 0);
    assert_eq!(view.window(30), (11, 30));
    view.scroll_newer(1, 30);
    assert_eq!(view.from_end, 0);
}

#[test]
fn test_scroll_newer_clamps_a_stale_from_end_first() {
    // `from_end` was set while the list was longer: it is clamped before stepping.
    let mut view = view_at(50);
    view.scroll_newer(1, 30);
    assert_eq!(view.from_end, 10);
}

#[test]
fn test_scroll_older_then_newer_is_symmetric_within_bounds() {
    let mut view = LogView::new();
    view.scroll_older(6, 100);
    assert_eq!(view.from_end, 6);
    view.scroll_newer(6, 100);
    assert_eq!(view.from_end, 0);
}

#[test]
fn test_show_history_switches_scope_and_resets_the_window() {
    let mut view = view_at(4);
    view.show_history(lines(5));
    assert_eq!(view.scope, LogScope::History);
    assert_eq!(view.from_end, 0);
    assert_eq!(view.history, lines(5));
}

#[test]
fn test_show_history_keeps_the_newest_capped_lines() {
    let mut view = LogView::new();
    view.show_history(lines(LOG_HISTORY_MAX_LINES + 3));
    assert_eq!(view.history.len(), LOG_HISTORY_MAX_LINES);
    assert_eq!(view.history.first().unwrap(), "line 3");
    assert_eq!(
        view.history.last().unwrap(),
        &format!("line {}", LOG_HISTORY_MAX_LINES + 2)
    );
}

#[test]
fn test_show_history_keeps_exactly_the_cap_untouched() {
    let mut view = LogView::new();
    view.show_history(lines(LOG_HISTORY_MAX_LINES));
    assert_eq!(view.history.len(), LOG_HISTORY_MAX_LINES);
    assert_eq!(view.history.first().unwrap(), "line 0");
}

#[test]
fn test_show_history_sanitizes_each_line() {
    let mut view = LogView::new();
    view.show_history(vec!["\u{1b}[31m  warn\u{7f}\t".to_owned()]);
    assert_eq!(view.history, vec!["[31m  warn".trim().to_owned()]);
}

#[test]
fn test_show_session_drops_the_history() {
    let mut view = LogView::new();
    view.show_history(lines(4));
    view.scroll_older(2, 40);
    view.show_session();
    assert_eq!(view.scope, LogScope::Session);
    assert!(view.history.is_empty());
    assert_eq!(view.from_end, 0);
}

#[test]
fn test_logged_line_is_appended_to_history_only_in_history_scope() {
    let mut view = LogView::new();
    view.on_line_logged("session only");
    assert!(view.history.is_empty());

    view.show_history(lines(2));
    view.on_line_logged("new");
    assert_eq!(view.history.last().unwrap(), "new");
    assert_eq!(view.history.len(), 3);
}

#[test]
fn test_history_cap_drops_the_oldest_line_when_exceeded() {
    let mut view = LogView::new();
    view.show_history(lines(LOG_HISTORY_MAX_LINES - 1));
    view.on_line_logged("fills it");
    // Exactly at the cap: nothing dropped yet.
    assert_eq!(view.history.len(), LOG_HISTORY_MAX_LINES);
    assert_eq!(view.history.first().unwrap(), "line 0");

    view.on_line_logged("one more");
    assert_eq!(view.history.len(), LOG_HISTORY_MAX_LINES);
    assert_eq!(view.history.first().unwrap(), "line 1");
    assert_eq!(view.history.last().unwrap(), "one more");
}

#[test]
fn test_scrolled_back_window_follows_new_lines() {
    // Not at the newest line: each logged line shifts the window by one so it keeps showing
    // the same lines.
    let mut view = view_at(0);
    view.on_line_logged("x");
    assert_eq!(view.from_end, 0, "at the bottom, nothing to keep in place");

    let mut view = view_at(5);
    view.on_line_logged("x");
    view.on_line_logged("y");
    assert_eq!(view.from_end, 7);
}

#[test]
fn test_sanitize_drops_control_characters_and_trims() {
    assert_eq!(sanitize_log_line("  hello \t"), "hello");
    assert_eq!(sanitize_log_line("a\u{1b}b\u{7f}c"), "abc");
    assert_eq!(sanitize_log_line("tab\tin"), "tabin");
    assert_eq!(sanitize_log_line("\r\n"), "");
    // Printable non-ASCII is kept.
    assert_eq!(sanitize_log_line("Français é"), "Français é");
    // Space (0x20) itself is printable and kept inside the line.
    assert_eq!(sanitize_log_line("a b"), "a b");
}
