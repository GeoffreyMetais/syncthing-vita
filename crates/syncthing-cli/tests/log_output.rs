//! Daemon output: control characters from peers are neutralized, and every line carries a
//! UTC `HH:MM:SS` timestamp.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use syncthing_cli::daemon::log::{log, printable, timestamp};

/// Environment variable telling the re-executed test binary to print a line with [`log`].
const CHILD_ENV: &str = "SYNCTHING_CLI_LOG_CHILD";

#[test]
fn printable_keeps_ordinary_text() {
    assert_eq!(printable("laptop (Vita) 42%"), "laptop (Vita) 42%");
    assert_eq!(printable("café / 東京"), "café / 東京");
    assert_eq!(printable(""), "");
}

#[test]
fn printable_replaces_each_control_character_with_a_question_mark() {
    assert_eq!(printable("a\nb"), "a?b");
    assert_eq!(printable("a\r\nb"), "a??b");
    assert_eq!(printable("\x1b[2J"), "?[2J");
    assert_eq!(printable("tab\there"), "tab?here");
    assert_eq!(printable("\x07\x00"), "??");
}

fn seconds_of_day(hh: u64, mm: u64, ss: u64) -> u64 {
    hh * 3600 + mm * 60 + ss
}

#[test]
fn timestamp_is_hh_mm_ss_of_the_current_utc_time() {
    let text = timestamp();
    let bytes = text.as_bytes();
    assert_eq!(bytes.len(), 8, "{text:?}");
    assert_eq!(bytes[2], b':', "{text:?}");
    assert_eq!(bytes[5], b':', "{text:?}");
    let field = |range: std::ops::Range<usize>| -> u64 {
        text[range]
            .parse()
            .unwrap_or_else(|_| panic!("digits in {text:?}"))
    };
    let (hh, mm, ss) = (field(0..2), field(3..5), field(6..8));
    assert!(hh < 24 && mm < 60 && ss < 60, "{text:?}");

    // Compare with the clock read right after: the value must be the current UTC time
    // (a little tolerance for the time between the two reads, and the day wrap-around).
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after 1970")
        .as_secs()
        % 86_400;
    let shown = seconds_of_day(hh, mm, ss);
    let diff = now.abs_diff(shown).min(86_400 - now.abs_diff(shown));
    assert!(
        diff <= 2,
        "timestamp {text} is not the current UTC time (now {now}s of day)"
    );
}

/// Runs this binary again with [`CHILD_ENV`] set and returns what it printed on stdout.
fn child_stdout() -> String {
    let output = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .expect("run child test");
    assert!(output.status.success(), "child failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The child half: prints one line through [`log`] when re-executed with [`CHILD_ENV`].
#[test]
fn child_entry() {
    if std::env::var(CHILD_ENV).is_ok() {
        log("peer name\x07 with\nnewline");
    }
}

#[test]
fn log_writes_one_timestamped_line_with_neutralized_text() {
    let out = child_stdout();
    let line = out
        .lines()
        .find(|l| l.contains("peer name"))
        .unwrap_or_else(|| panic!("no log line in {out:?}"));
    let start = line.find('[').expect("timestamp bracket");
    let line = &line[start..];
    assert_eq!(
        line.len(),
        "[HH:MM:SS] ".len() + "peer name? with?newline".len(),
        "{line:?}"
    );
    assert!(line.starts_with('['), "{line:?}");
    assert_eq!(&line[9..11], "] ", "{line:?}");
    assert_eq!(&line[11..], "peer name? with?newline", "{line:?}");
}
