//! Daemon output: one timestamped line per event on stdout.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// `HH:MM:SS` (UTC) of now.
pub fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(
        "{:02}:{:02}:{:02}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

/// Replaces control characters: names, labels and file names come from peers and must not
/// inject terminal escape sequences or fake log lines.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Prints `message` with a timestamp (one write per line: lines of concurrent threads never
/// interleave).
pub fn log(message: &str) {
    let line = format!("[{}] {}\n", timestamp(), printable(message));
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();
}
