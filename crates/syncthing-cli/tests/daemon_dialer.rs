//! The dialer of a configured device: what it logs when it knows no address or cannot connect,
//! and when it asks Global Discovery for the device's addresses.

mod common;

use std::time::Duration;

use syncthing_core::DeviceIdentity;

use common::servers::{
    https_url, isolate_proxy_env, request_count, save_peer_config, spawn_https_service,
    spawn_tcp_service, HttpReply, HttpRequest,
};
use common::{free_port, temp_home, valid_device_id, Cli};

/// Requests received by a test server.
type RequestLog = std::sync::Arc<std::sync::Mutex<Vec<HttpRequest>>>;

/// A closed loopback port: connecting to it is refused at once.
fn closed_address() -> String {
    format!("tcp://127.0.0.1:{}", free_port())
}

/// A Global Discovery lookup stand-in answering every query with `addresses`. Returns its URL
/// and the request log.
fn lookup_server(addresses: &[String]) -> (String, RequestLog) {
    let identity = DeviceIdentity::from_seed(&[0x71; 32]).expect("server identity");
    let body = format!(
        "{{\"addresses\":[{}]}}",
        addresses
            .iter()
            .map(|a| format!("\"{a}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let (port, log) = spawn_https_service(&identity, false, move |_| HttpReply::ok(&body));
    (https_url(port, &identity, "noannounce"), log)
}

#[test]
fn dialer_says_once_that_it_waits_for_discovery_when_a_device_has_no_address() {
    isolate_proxy_env();
    let home = temp_home("dialer-no-address");
    let laptop = valid_device_id(101);
    save_peer_config(&home, &laptop, &["dynamic"], &[], &[]);
    let daemon = Cli::new(&home).spawn_daemon(&[
        "--listen",
        "none",
        "--no-local-discovery",
        "--no-global-discovery",
        "--no-relays",
    ]);

    daemon.wait_for_log_line(
        "No address known for laptop yet, waiting for discovery",
        Duration::from_secs(10),
    );
}

#[test]
fn dialer_reports_a_failed_connection_with_its_endpoint() {
    isolate_proxy_env();
    let home = temp_home("dialer-refused");
    let laptop = valid_device_id(102);
    let endpoint = closed_address();
    save_peer_config(&home, &laptop, &[endpoint.as_str()], &[], &[]);
    let daemon = Cli::new(&home).spawn_daemon(&[
        "--listen",
        "none",
        "--no-local-discovery",
        "--no-global-discovery",
        "--no-relays",
    ]);

    let expected = format!("Cannot sync with laptop at {endpoint}:");
    daemon.wait_for_log_line(&expected, Duration::from_secs(10));
    // The same error is not repeated while it stays the same.
    std::thread::sleep(Duration::from_millis(1500));
    let count = daemon
        .stdout_lines()
        .iter()
        .filter(|l| l.contains(&expected))
        .count();
    assert_eq!(count, 1, "stdout:\n{}", daemon.stdout());
}

#[test]
fn dialer_dials_the_endpoints_global_discovery_returns() {
    isolate_proxy_env();
    let home = temp_home("dialer-lookup");
    let laptop = valid_device_id(103);
    let endpoint = closed_address();
    let (lookup, log) = lookup_server(std::slice::from_ref(&endpoint));
    save_peer_config(&home, &laptop, &["dynamic"], &[], &[lookup]);
    let daemon =
        Cli::new(&home).spawn_daemon(&["--listen", "none", "--no-local-discovery", "--no-relays"]);

    daemon.wait_for_log_line(
        &format!("Cannot sync with laptop at {endpoint}:"),
        Duration::from_secs(15),
    );
    assert_eq!(request_count(&log), 1, "one lookup is enough");
}

#[test]
fn dialer_looks_a_device_up_once_per_interval_not_on_every_poll() {
    isolate_proxy_env();
    let home = temp_home("dialer-lookup-interval");
    let laptop = valid_device_id(104);
    // Nothing is found: the dialer polls for an address every two seconds.
    let (lookup, log) = lookup_server(&[]);
    save_peer_config(&home, &laptop, &["dynamic"], &[], &[lookup]);
    let daemon =
        Cli::new(&home).spawn_daemon(&["--listen", "none", "--no-local-discovery", "--no-relays"]);

    daemon.wait_for_log_line("waiting for discovery", Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(
        request_count(&log),
        1,
        "the lookup is repeated once a minute, not on every poll"
    );
}

#[test]
fn dialer_does_not_look_devices_up_without_global_discovery() {
    isolate_proxy_env();
    let home = temp_home("dialer-lookup-off");
    let laptop = valid_device_id(105);
    let endpoint = closed_address();
    let (lookup, log) = lookup_server(std::slice::from_ref(&endpoint));
    save_peer_config(&home, &laptop, &["dynamic"], &[], &[lookup]);
    let daemon = Cli::new(&home).spawn_daemon(&[
        "--listen",
        "none",
        "--no-local-discovery",
        "--no-global-discovery",
        "--no-relays",
    ]);

    daemon.wait_for_log_line("waiting for discovery", Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        request_count(&log),
        0,
        "no lookup when Global Discovery is off"
    );
    assert!(
        !daemon.stdout().contains("Cannot sync with laptop"),
        "stdout:\n{}",
        daemon.stdout()
    );
}

/// A connection that fails is dialed again after the error delay (about ten seconds): the test
/// waits for the second attempt, which takes that long.
#[test]
fn dialer_dials_again_after_a_failed_connection() {
    isolate_proxy_env();
    let home = temp_home("dialer-retry");
    let laptop = valid_device_id(106);
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = std::sync::Arc::clone(&attempts);
    // Accepts each connection and hangs up at once: the TLS handshake fails.
    let port = spawn_tcp_service(move |tcp| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        drop(tcp);
    });
    save_peer_config(
        &home,
        &laptop,
        &[format!("tcp://127.0.0.1:{port}").as_str()],
        &[],
        &[],
    );
    let daemon = Cli::new(&home).spawn_daemon(&[
        "--listen",
        "none",
        "--no-local-discovery",
        "--no-global-discovery",
        "--no-relays",
    ]);

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while attempts.load(std::sync::atomic::Ordering::SeqCst) < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "no second attempt within 15 s\n{}",
            daemon.stdout()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
