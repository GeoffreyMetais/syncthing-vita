//! The Rust mirror of the launcher / worker contract (`src/link.rs`) agrees with the C header
//! the launcher is built with (`plugin/include/syncthing_plugin.h`).

use std::collections::HashMap;
use std::mem::{offset_of, size_of};

use syncthing_vita_worker::link::{self, outcome_code, WorkerLink};
use syncthing_vita_worker::CycleOutcome;

/// `#define NAME VALUE` lines of the header (value without comment).
fn header_defines() -> HashMap<String, String> {
    let header = include_str!("../../../plugin/include/syncthing_plugin.h");
    header
        .lines()
        .filter_map(|line| line.trim().strip_prefix("#define "))
        .filter_map(|rest| {
            let (name, value) = rest.split_once(char::is_whitespace)?;
            let value = value.split("/*").next()?.trim();
            Some((name.to_owned(), value.to_owned()))
        })
        .collect()
}

fn number(defines: &HashMap<String, String>, name: &str) -> i64 {
    let raw = defines
        .get(name)
        .unwrap_or_else(|| panic!("{name} missing from the header"));
    let raw = raw.trim_end_matches('u');
    match raw.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).unwrap(),
        None => raw.parse().unwrap(),
    }
}

fn string(defines: &HashMap<String, String>, name: &str) -> String {
    defines[name].trim_matches('"').to_owned()
}

#[test]
fn test_names_agree() {
    let d = header_defines();
    assert_eq!(string(&d, "STP_LAUNCHER_FILE"), link::LAUNCHER_FILE);
    assert_eq!(string(&d, "STP_WORKER_FILE"), link::WORKER_FILE);
    assert_eq!(string(&d, "STP_GUI_RUNNING_MUTEX"), link::GUI_RUNNING_MUTEX);
    assert_eq!(
        string(&d, "STP_LAUNCHER_ALIVE_MUTEX"),
        link::LAUNCHER_ALIVE_MUTEX
    );
    assert_eq!(
        string(&d, "STP_WORKER_RUNNING_MUTEX"),
        link::WORKER_RUNNING_MUTEX
    );
    assert_eq!(
        string(&d, "STP_DATA_DIR"),
        syncthing_vita_worker::defaults::VITA_DATA_DIR
    );
}

#[test]
fn test_codes_agree() {
    let d = header_defines();
    assert_eq!(number(&d, "STP_LINK_MAGIC"), i64::from(link::LINK_MAGIC));
    assert_eq!(
        number(&d, "STP_LINK_VERSION"),
        i64::from(link::LINK_VERSION)
    );
    for (name, value) in [
        ("STP_STATE_LOADING", link::STATE_LOADING),
        ("STP_STATE_RUNNING", link::STATE_RUNNING),
        ("STP_STATE_DONE", link::STATE_DONE),
        ("STP_STATE_FATAL", link::STATE_FATAL),
    ] {
        assert_eq!(number(&d, name), i64::from(value), "{name}");
    }
    for (name, value) in [
        ("STP_RESULT_COMPLETED", link::RESULT_COMPLETED),
        ("STP_RESULT_ABORTED", link::RESULT_ABORTED),
        ("STP_RESULT_DEADLINE", link::RESULT_DEADLINE),
        ("STP_RESULT_DISABLED", link::RESULT_DISABLED),
        ("STP_RESULT_NOTHING_TO_SYNC", link::RESULT_NOTHING_TO_SYNC),
        ("STP_RESULT_NOT_CONFIGURED", link::RESULT_NOT_CONFIGURED),
        ("STP_RESULT_NO_NETWORK", link::RESULT_NO_NETWORK),
        ("STP_RESULT_NO_MEMORY", link::RESULT_NO_MEMORY),
        ("STP_RESULT_BAD_LINK", link::RESULT_BAD_LINK),
        ("STP_RESULT_NO_THREAD", link::RESULT_NO_THREAD),
    ] {
        assert_eq!(number(&d, name), i64::from(value), "{name}");
    }
}

#[test]
fn test_link_layout_matches_the_c_struct() {
    // Five 32-bit fields, in the header's order.
    assert_eq!(size_of::<WorkerLink>(), 20);
    assert_eq!(offset_of!(WorkerLink, magic), 0);
    assert_eq!(offset_of!(WorkerLink, version), 4);
    assert_eq!(offset_of!(WorkerLink, abort), 8);
    assert_eq!(offset_of!(WorkerLink, state), 12);
    assert_eq!(offset_of!(WorkerLink, result), 16);
}

#[test]
fn test_link_compatibility_checks_magic_and_version() {
    use std::sync::atomic::{AtomicI32, AtomicU32};
    let make = |magic, version| WorkerLink {
        magic,
        version,
        abort: AtomicU32::new(0),
        state: AtomicU32::new(0),
        result: AtomicI32::new(0),
    };
    assert!(make(link::LINK_MAGIC, link::LINK_VERSION).is_compatible());
    assert!(!make(link::LINK_MAGIC ^ 1, link::LINK_VERSION).is_compatible());
    assert!(!make(link::LINK_MAGIC, link::LINK_VERSION + 1).is_compatible());
}

#[test]
fn test_outcome_codes() {
    assert_eq!(outcome_code(&CycleOutcome::Completed), 0);
    assert_eq!(outcome_code(&CycleOutcome::Aborted), 1);
    assert_eq!(outcome_code(&CycleOutcome::DeadlineReached), 2);
    assert_eq!(outcome_code(&CycleOutcome::Disabled), 3);
    assert_eq!(outcome_code(&CycleOutcome::NothingToSync), 4);
    assert_eq!(
        outcome_code(&CycleOutcome::NotConfigured("x".to_owned())),
        5
    );
}
