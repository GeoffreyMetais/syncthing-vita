//! Log lines built from session events: byte counts, shell quoting of peer-supplied names and
//! the "in sync" report, whose wording and frequency are the CLI's user interface.

mod common;

use std::process::Command;

use syncthing_cli::config::Config;
use syncthing_cli::daemon::context::{DaemonContext, DaemonOptions, KnownPeer};
use syncthing_cli::daemon::events::{handle_event, human_bytes, shell_word};
use syncthing_core::{DeviceIdentity, HttpsClient, SyncEvent};

use common::temp_home;

/// Environment variable telling the re-executed test binary which scenario to run.
const CHILD_ENV: &str = "SYNCTHING_CLI_EVENTS_CHILD";

#[test]
fn human_bytes_below_one_kibibyte_is_exact() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(1), "1 B");
    assert_eq!(human_bytes(500), "500 B");
    assert_eq!(human_bytes(1023), "1023 B");
}

#[test]
fn human_bytes_switches_units_at_powers_of_1024() {
    assert_eq!(human_bytes(1024), "1.0 KiB");
    assert_eq!(human_bytes(1536), "1.5 KiB");
    assert_eq!(human_bytes(1024 * 1024 - 1), "1024.0 KiB");
    assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
    assert_eq!(human_bytes(3 * 1024 * 1024 + 512 * 1024), "3.5 MiB");
    assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GiB");
    assert_eq!(human_bytes(1024u64.pow(4)), "1.0 TiB");
}

#[test]
fn human_bytes_stops_at_tebibytes() {
    // Beyond TiB the value keeps growing in TiB instead of overflowing the unit table.
    assert_eq!(human_bytes(1024u64.pow(5)), "1024.0 TiB");
    assert_eq!(human_bytes(u64::MAX), "16777216.0 TiB");
}

#[test]
fn shell_word_leaves_plain_words_unquoted() {
    assert_eq!(shell_word("abc"), "abc");
    assert_eq!(shell_word("photos-2024_v1.bak"), "photos-2024_v1.bak");
}

#[test]
fn shell_word_quotes_empty_and_special_words() {
    assert_eq!(shell_word(""), "''");
    assert_eq!(shell_word("my folder"), "'my folder'");
    assert_eq!(shell_word("a;rm -rf"), "'a;rm -rf'");
    assert_eq!(shell_word("$HOME"), "'$HOME'");
}

#[test]
fn shell_word_escapes_single_quotes() {
    assert_eq!(shell_word("it's"), "'it'\\''s'");
    assert_eq!(shell_word("'"), "''\\'''");
}

fn context() -> DaemonContext {
    let identity = DeviceIdentity::from_seed(&[7; 32]).expect("identity");
    let options = DaemonOptions {
        listen: None,
        local_discovery: false,
        global_discovery: false,
        relays: false,
        announce_servers: Vec::new(),
        lookup_servers: Vec::new(),
        relay_pool_url: String::new(),
        static_relays: Vec::new(),
        https: HttpsClient::default(),
    };
    DaemonContext::new(
        &temp_home("events-format"),
        &Config::default(),
        identity,
        options,
    )
}

fn peer() -> KnownPeer {
    KnownPeer {
        id: DeviceIdentity::from_seed(&[9; 32])
            .expect("peer identity")
            .device_id,
        name: "laptop".into(),
        addresses: Vec::new(),
    }
}

/// A completed sync cycle of `peer()` with the given transfer counters.
fn cycle(files_downloaded: usize, bytes_downloaded: u64, bytes_uploaded: u64) -> SyncEvent {
    SyncEvent::SyncCycleCompleted {
        peer_device_id: peer().id,
        active_connections: 1,
        files_up_to_date: 0,
        files_downloaded,
        bytes_downloaded,
        blocks_uploaded: 0,
        bytes_uploaded,
    }
}

/// Runs `scenario` in a child process of this test binary and returns its log lines (the
/// lines written by `log`, which go straight to stdout).
fn captured_logs(scenario: &str) -> Vec<String> {
    let output = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, scenario)
        .output()
        .expect("run child test");
    assert!(
        output.status.success(),
        "child scenario {scenario} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.find('[').map(|i| line[i..].to_owned()))
        .collect()
}

/// The scenarios run by the child process (see [`captured_logs`]).
fn run_scenario(scenario: &str) {
    let ctx = context();
    let peer = peer();
    match scenario {
        "first_quiet" => handle_event(&ctx, &peer, cycle(0, 0, 0)),
        "second_quiet" => {
            handle_event(&ctx, &peer, cycle(0, 0, 0));
            handle_event(&ctx, &peer, cycle(0, 0, 0));
        }
        "second_download" => {
            handle_event(&ctx, &peer, cycle(0, 0, 0));
            handle_event(&ctx, &peer, cycle(2, 4096, 0));
        }
        "second_upload" => {
            handle_event(&ctx, &peer, cycle(0, 0, 0));
            handle_event(&ctx, &peer, cycle(0, 0, 5));
        }
        "download_only_bytes" => {
            handle_event(&ctx, &peer, cycle(0, 0, 0));
            handle_event(&ctx, &peer, cycle(0, 7, 0));
        }
        other => panic!("unknown scenario {other}"),
    }
}

/// The child half: runs only when re-executed with [`CHILD_ENV`] set.
#[test]
fn child_entry() {
    if let Ok(scenario) = std::env::var(CHILD_ENV) {
        run_scenario(&scenario);
    }
}

fn in_sync_lines(scenario: &str) -> Vec<String> {
    captured_logs(scenario)
        .into_iter()
        .filter(|l| l.contains("In sync with laptop"))
        .collect()
}

#[test]
fn first_cycle_is_always_reported() {
    let lines = in_sync_lines("first_quiet");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].ends_with("In sync with laptop (session: 0 file(s), 0 B received, 0 B sent)"),
        "{}",
        lines[0]
    );
}

#[test]
fn quiet_repeated_cycles_are_not_reported() {
    let lines = in_sync_lines("second_quiet");
    assert_eq!(
        lines.len(),
        1,
        "only the first cycle is reported: {lines:?}"
    );
}

#[test]
fn repeated_cycle_with_downloads_is_reported() {
    let lines = in_sync_lines("second_download");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[1].ends_with("In sync with laptop (session: 2 file(s), 4.0 KiB received, 0 B sent)"),
        "{}",
        lines[1]
    );
}

#[test]
fn repeated_cycle_with_uploads_is_reported() {
    let lines = in_sync_lines("second_upload");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[1].ends_with("In sync with laptop (session: 0 file(s), 0 B received, 5 B sent)"),
        "{}",
        lines[1]
    );
}

#[test]
fn repeated_cycle_with_received_bytes_only_is_not_reported() {
    // Only files and uploads trigger a report, not the bytes received alone.
    let lines = in_sync_lines("download_only_bytes");
    assert_eq!(lines.len(), 1, "{lines:?}");
}
