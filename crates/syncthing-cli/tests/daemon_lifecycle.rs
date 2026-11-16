//! How the daemon starts and stops: the wait for its threads at shutdown, the exit when its
//! time is up, and the network features the `run` command reports.

mod common;

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use syncthing_cli::config::{load_config, save_config};

use common::servers::{isolate_proxy_env, save_peer_config, spawn_tcp_service};
use common::{free_port, temp_home, valid_device_id, Cli, LOCAL_ONLY};

/// Flags of a daemon that uses no network feature at all, besides what each test adds.
fn quiet_flags() -> Vec<&'static str> {
    let mut flags = vec!["--exit-after", "1"];
    flags.extend_from_slice(LOCAL_ONLY);
    flags
}

#[test]
fn daemon_stops_at_once_when_its_only_threads_end() {
    isolate_proxy_env();
    let home = temp_home("lifecycle-listener");
    let listen = format!("127.0.0.1:{}", free_port());
    let mut args = vec!["--listen", listen.as_str()];
    args.extend(quiet_flags());
    let mut daemon = Cli::new(&home).spawn_daemon(&args);

    let started = Instant::now();
    let status = daemon.wait_for_exit(Duration::from_secs(20));
    let elapsed = started.elapsed();
    assert!(status.success(), "exit: {status}\n{}", daemon.stdout());
    // The listener notices the stop within one poll step: no shutdown wait is needed.
    assert!(
        elapsed < Duration::from_secs(4),
        "the daemon took {elapsed:?} to stop after its time was up"
    );
}

#[test]
fn daemon_waits_a_bounded_time_for_a_thread_stuck_in_a_connection() {
    isolate_proxy_env();
    let home = temp_home("lifecycle-stuck");
    // A device whose address accepts the connection and never answers the TLS handshake.
    let silent_port = spawn_tcp_service(|_tcp| thread::sleep(Duration::from_secs(30)));
    let laptop = valid_device_id(131);
    save_peer_config(
        &home,
        &laptop,
        &[format!("tcp://127.0.0.1:{silent_port}").as_str()],
        &[],
        &[],
    );
    let mut daemon = Cli::new(&home).spawn_daemon(&quiet_flags());

    let started = Instant::now();
    let status = daemon.wait_for_exit(Duration::from_secs(20));
    let elapsed = started.elapsed();
    assert!(status.success(), "exit: {status}\n{}", daemon.stdout());
    // The dialer is stuck for its connection timeout (10 s). The daemon waits for it for its
    // shutdown grace period (5 s) and no longer.
    assert!(
        elapsed >= Duration::from_millis(4500),
        "the daemon did not wait for its threads: stopped after {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(8500),
        "the daemon waited longer than its shutdown grace period: {elapsed:?}"
    );
    assert!(
        daemon.stdout().contains("Stopped"),
        "stdout:\n{}",
        daemon.stdout()
    );
}

/// `run` tells which HTTPS proxy Global Discovery and the relay pool go through, when either
/// of them runs.
#[test]
fn run_names_the_https_proxy_when_relays_run_without_global_discovery() {
    isolate_proxy_env();
    let home = temp_home("run-notes");
    let laptop = valid_device_id(132);
    save_peer_config(&home, &laptop, &["dynamic"], &[], &[]);
    // The relay is a closed port: the join fails at once and nothing is fetched from the pool.
    let mut config = load_config(&home).expect("load configuration");
    config.static_relays = vec![format!("relay://127.0.0.1:{}/?id={laptop}", free_port())];
    save_config(&home, &config).expect("save configuration");

    let output = Command::new(env!("CARGO_BIN_EXE_syncthing-cli"))
        .arg("--home")
        .arg(&home)
        .args([
            "run",
            "--listen",
            "none",
            "--exit-after",
            "1",
            "--no-local-discovery",
            "--no-global-discovery",
        ])
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env_remove("https_proxy")
        .env_remove("ALL_PROXY")
        .env_remove("all_proxy")
        .env_remove("SSL_CERT_FILE")
        .output()
        .expect("run syncthing-cli");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("HTTPS proxy 127.0.0.1:9 (from HTTPS_PROXY)"),
        "stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
