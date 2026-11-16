//! `syncthing.log` persistence: line format, rotation above `LOG_MAX_BYTES` into
//! `syncthing.log.1`, and the bounded history read back by the log viewer.

mod common;

use std::fs;

use common::temp_test_dir;
use vita_syncthing_app::domain::defaults::LOG_MAX_BYTES;
use vita_syncthing_app::infrastructure::ConfigRepository;

const PREFIX: &str = "[VITA-SYNCTHING] ";

/// Length of the prefix plus the newline added to every log line.
const LINE_OVERHEAD: usize = PREFIX.len() + 1;

fn log_path(dir: &str) -> String {
    format!("{dir}/syncthing.log")
}

fn rotated_path(dir: &str) -> String {
    format!("{dir}/syncthing.log.1")
}

#[test]
fn test_log_line_format_is_prefixed_and_newline_terminated() {
    let dir = temp_test_dir("log-format");
    let repo = ConfigRepository::new(&dir);
    repo.write_log("first");
    repo.write_log("second line");
    assert_eq!(
        fs::read_to_string(log_path(&dir)).unwrap(),
        "[VITA-SYNCTHING] first\n[VITA-SYNCTHING] second line\n"
    );
    assert!(fs::metadata(rotated_path(&dir)).is_err());
}

#[test]
fn test_write_log_creates_missing_data_directory() {
    let root = temp_test_dir("log-create-dir");
    let dir = format!("{root}/fresh/install");
    ConfigRepository::new(&dir).write_log("hello");
    assert_eq!(
        fs::read_to_string(log_path(&dir)).unwrap(),
        "[VITA-SYNCTHING] hello\n"
    );
}

#[test]
fn test_log_at_exactly_the_limit_is_not_rotated() {
    let dir = temp_test_dir("log-limit-exact");
    let repo = ConfigRepository::new(&dir);
    let message = "x".repeat(LOG_MAX_BYTES as usize - LINE_OVERHEAD);
    repo.write_log(&message);
    let len = fs::metadata(log_path(&dir)).unwrap().len();
    assert_eq!(len, LOG_MAX_BYTES, "the test line fills the log exactly");
    assert!(
        fs::metadata(rotated_path(&dir)).is_err(),
        "not above the limit"
    );
}

#[test]
fn test_log_above_the_limit_is_rotated_to_previous_file() {
    let dir = temp_test_dir("log-rotate");
    let repo = ConfigRepository::new(&dir);
    // An older rotated file is replaced, not kept.
    fs::write(rotated_path(&dir), "old history\n").unwrap();

    // The first line leaves the log just under the limit: no rotation yet.
    let first = "y".repeat(LOG_MAX_BYTES as usize - LINE_OVERHEAD - 100);
    repo.write_log(&first);
    assert!(fs::metadata(log_path(&dir)).is_ok());
    assert!(fs::read_to_string(rotated_path(&dir))
        .unwrap()
        .contains("old history"));

    // The second line crosses the limit: the whole log becomes `syncthing.log.1`.
    let second = "w".repeat(200);
    repo.write_log(&second);
    assert!(
        fs::metadata(log_path(&dir)).is_err(),
        "the full log was moved away"
    );
    let rotated = fs::read_to_string(rotated_path(&dir)).unwrap();
    assert!(!rotated.contains("old history"));
    assert_eq!(rotated, format!("{PREFIX}{first}\n{PREFIX}{second}\n"));

    repo.write_log("after rotation");
    assert_eq!(
        fs::read_to_string(log_path(&dir)).unwrap(),
        "[VITA-SYNCTHING] after rotation\n"
    );
}

#[test]
fn test_history_reads_rotated_then_current_file_oldest_first() {
    let dir = temp_test_dir("log-history-order");
    let repo = ConfigRepository::new(&dir);
    let big = "z".repeat(LOG_MAX_BYTES as usize);
    repo.write_log(&big);
    repo.write_log("next");
    repo.write_log("after");

    let history = repo.read_log_history(10);
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].len(), big.len(), "prefix stripped, text kept");
    assert_eq!(history[1], "next");
    assert_eq!(history[2], "after");
}

#[test]
fn test_history_keeps_only_the_most_recent_lines() {
    let dir = temp_test_dir("log-history-tail");
    let repo = ConfigRepository::new(&dir);
    for i in 1..=5 {
        repo.write_log(&format!("line {i}"));
    }
    assert_eq!(repo.read_log_history(2), vec!["line 4", "line 5"]);
    assert_eq!(
        repo.read_log_history(5),
        vec!["line 1", "line 2", "line 3", "line 4", "line 5"]
    );
    assert_eq!(repo.read_log_history(50).len(), 5);
    assert!(repo.read_log_history(0).is_empty());
}

#[test]
fn test_history_tail_spans_both_files() {
    let dir = temp_test_dir("log-history-span");
    let repo = ConfigRepository::new(&dir);
    fs::write(
        rotated_path(&dir),
        "[VITA-SYNCTHING] a\n[VITA-SYNCTHING] b\n",
    )
    .unwrap();
    fs::write(log_path(&dir), "[VITA-SYNCTHING] c\n[VITA-SYNCTHING] d\n").unwrap();
    assert_eq!(repo.read_log_history(3), vec!["b", "c", "d"]);
    assert_eq!(repo.read_log_history(1), vec!["d"]);
}

#[test]
fn test_history_keeps_lines_without_prefix_verbatim() {
    let dir = temp_test_dir("log-history-raw");
    let repo = ConfigRepository::new(&dir);
    fs::write(log_path(&dir), "plain text\n[VITA-SYNCTHING] prefixed\n\n").unwrap();
    assert_eq!(
        repo.read_log_history(10),
        vec!["plain text", "prefixed", ""]
    );
}

#[test]
fn test_history_of_missing_log_is_empty() {
    let dir = temp_test_dir("log-history-none");
    assert!(ConfigRepository::new(&dir).read_log_history(100).is_empty());
}
