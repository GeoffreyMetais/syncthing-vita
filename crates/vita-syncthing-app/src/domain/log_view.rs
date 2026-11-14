//! Full-screen protocol log viewer (`LogView`), opened from the `Settings` log card.
//!
//! It shows either the lines logged during this session (`AppState::logs`) or, after
//! `Show All`, the whole `syncthing.log` history read from disk. The window is anchored on
//! the newest line: `from_end` counts the lines hidden below it, so at `0` new lines scroll
//! into view as they are logged, and a scrolled-back window stays on the lines being read.

use crate::domain::defaults::LOG_HISTORY_MAX_LINES;

/// Log lines visible at once in the viewer.
pub const LOG_VIEW_ROWS: usize = 19;
/// Lines moved by a page (`L` / `R`, touching the upper / lower half of the list): one line
/// of the previous page stays visible for context.
pub const LOG_VIEW_PAGE: usize = LOG_VIEW_ROWS - 1;

/// Which lines the viewer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogScope {
    /// Lines logged since the app started (`AppState::logs`).
    Session,
    /// The whole `syncthing.log` history (`LogView::history`).
    History,
}

/// State of the open log viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogView {
    pub scope: LogScope,
    /// `syncthing.log` lines (oldest first) while `scope` is `History`, else empty.
    pub history: Vec<String>,
    /// Lines hidden below the window (`0`: the newest line is the last one shown).
    pub from_end: usize,
}

impl Default for LogView {
    fn default() -> Self {
        Self::new()
    }
}

impl LogView {
    /// Viewer on this session's lines, showing the newest ones.
    pub fn new() -> Self {
        Self {
            scope: LogScope::Session,
            history: Vec::new(),
            from_end: 0,
        }
    }

    /// Lines of the current scope, oldest first (`session` is `AppState::logs`).
    pub fn lines<'a>(&'a self, session: &'a [String]) -> &'a [String] {
        match self.scope {
            LogScope::Session => session,
            LogScope::History => &self.history,
        }
    }

    /// Visible range `start..end` among `total` lines.
    pub fn window(&self, total: usize) -> (usize, usize) {
        let end = total - self.from_end.min(Self::max_from_end(total));
        (end.saturating_sub(LOG_VIEW_ROWS), end)
    }

    /// Moves the window `n` lines towards older lines.
    pub fn scroll_older(&mut self, n: usize, total: usize) {
        self.from_end =
            (self.from_end.min(Self::max_from_end(total)) + n).min(Self::max_from_end(total));
    }

    /// Moves the window `n` lines towards newer lines.
    pub fn scroll_newer(&mut self, n: usize, total: usize) {
        self.from_end = self
            .from_end
            .min(Self::max_from_end(total))
            .saturating_sub(n);
    }

    /// Shows the `syncthing.log` history (oldest first), keeping its last
    /// `LOG_HISTORY_MAX_LINES` lines.
    pub fn show_history(&mut self, mut lines: Vec<String>) {
        if lines.len() > LOG_HISTORY_MAX_LINES {
            lines.drain(..lines.len() - LOG_HISTORY_MAX_LINES);
        }
        self.history = lines.iter().map(|l| sanitize_log_line(l)).collect();
        self.scope = LogScope::History;
        self.from_end = 0;
    }

    /// Back to this session's lines (the history is dropped).
    pub fn show_session(&mut self) {
        self.history = Vec::new();
        self.scope = LogScope::Session;
        self.from_end = 0;
    }

    /// A line was just logged: the history follows it, and a scrolled-back window keeps
    /// showing the same lines.
    pub fn on_line_logged(&mut self, line: &str) {
        if self.scope == LogScope::History {
            self.history.push(line.to_owned());
            if self.history.len() > LOG_HISTORY_MAX_LINES {
                self.history.remove(0);
            }
        }
        if self.from_end > 0 {
            self.from_end += 1;
        }
    }

    fn max_from_end(total: usize) -> usize {
        total.saturating_sub(LOG_VIEW_ROWS)
    }
}

/// A log line as displayed: control characters removed, surrounding spaces trimmed.
pub fn sanitize_log_line(line: &str) -> String {
    let clean: String = line
        .chars()
        .filter(|&c| c >= ' ' && c != '\u{7f}')
        .collect();
    clean.trim().to_owned()
}
