//! The worker's lines in the app's `syncthing.log`: prefix, sanitizing and rotation.

mod common;

use syncthing_vita_worker::log::WorkerLog;

#[test]
fn test_lines_have_the_app_prefix_and_no_control_characters() {
    let dir = common::fresh_temp_dir("log_prefix");
    let log = WorkerLog::new(&dir);
    log.line("first");
    log.line("forged\n[VITA-SYNCTHING] line\r\tend");
    let text = std::fs::read_to_string(dir.join("syncthing.log")).unwrap();
    assert_eq!(
        text,
        "[VITA-SYNCTHING] Background sync: first\n\
         [VITA-SYNCTHING] Background sync: forged [VITA-SYNCTHING] line  end\n"
    );
    // The app's log viewer reads them back without the prefix.
    let history = common::repo(&dir).read_log_history(10);
    assert_eq!(history[0], "Background sync: first");
}

#[test]
fn test_rotation_above_the_limit() {
    let dir = common::fresh_temp_dir("log_rotation");
    std::fs::write(dir.join("syncthing.log.1"), "older\n").unwrap();
    let line_len = "[VITA-SYNCTHING] Background sync: 0123456789\n".len() as u64;
    let log = WorkerLog::with_limit(&dir, 2 * line_len);
    log.line("0123456789");
    log.line("0123456789");
    assert!(
        dir.join("syncthing.log").exists(),
        "exactly at the limit: kept"
    );
    log.line("0123456789");
    assert!(
        !dir.join("syncthing.log").exists(),
        "above the limit: rotated"
    );
    let rotated = std::fs::read_to_string(dir.join("syncthing.log.1")).unwrap();
    assert_eq!(
        rotated.lines().count(),
        3,
        "the previous rotated log is replaced"
    );
    log.line("next");
    assert_eq!(
        std::fs::read_to_string(dir.join("syncthing.log")).unwrap(),
        "[VITA-SYNCTHING] Background sync: next\n"
    );
}

#[test]
fn test_default_limit_is_the_app_one() {
    assert_eq!(syncthing_vita_worker::defaults::LOG_MAX_BYTES, 512 * 1024);
    let dir = common::fresh_temp_dir("log_missing_dir").join("missing");
    // No directory: nothing is written and nothing fails.
    WorkerLog::new(&dir).line("lost");
    assert!(!dir.exists());
}
