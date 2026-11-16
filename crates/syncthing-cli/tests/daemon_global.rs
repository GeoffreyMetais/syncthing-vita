//! Global Discovery announcements of the daemon: when it announces, what it announces, and how
//! often.

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use syncthing_core::DeviceIdentity;

use common::servers::{
    drain_frames, https_url, isolate_proxy_env, read_frame, request_count, response_frame,
    save_peer_config, spawn_https_service, spawn_tls_service, HttpReply, HttpRequest, RELAY_ALPN,
};
use common::{free_port, temp_home, valid_device_id, Cli, Daemon};

/// Requests received by a test server.
type RequestLog = Arc<Mutex<Vec<HttpRequest>>>;

/// Announce server presenting seed `0x81`, answering `Reannounce-After: 3600`.
fn announce_server() -> (String, RequestLog) {
    let identity = DeviceIdentity::from_seed(&[0x81; 32]).expect("server identity");
    let (port, log) = spawn_https_service(&identity, true, |_| {
        HttpReply::ok("").with_header("Reannounce-After", "3600")
    });
    let url = https_url(port, &identity, "nolookup");
    (url, log)
}

/// Starts a daemon of `home` announcing to `announce`, with no relays and no local discovery.
fn spawn_announcing_daemon(home: &std::path::Path, extra: &[&str]) -> Daemon {
    let mut args = vec!["--no-local-discovery", "--no-relays"];
    args.extend_from_slice(extra);
    Cli::new(home).spawn_daemon(&args)
}

#[test]
fn daemon_announces_nothing_while_it_has_no_address_to_announce() {
    isolate_proxy_env();
    let home = temp_home("announce-none");
    let (announce, log) = announce_server();
    save_peer_config(&home, &valid_device_id(111), &["dynamic"], &[announce], &[]);
    let _daemon = spawn_announcing_daemon(&home, &["--listen", "none"]);

    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        request_count(&log),
        0,
        "an empty announcement is never sent"
    );
}

#[test]
fn daemon_announces_its_listening_address_once_per_reannounce_interval() {
    isolate_proxy_env();
    let home = temp_home("announce-once");
    let (announce, log) = announce_server();
    save_peer_config(&home, &valid_device_id(112), &["dynamic"], &[announce], &[]);
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let daemon = spawn_announcing_daemon(&home, &["--listen", &listen]);
    daemon.wait_for_log_line("Listening on", Duration::from_secs(10));

    let deadline = Instant::now() + Duration::from_secs(10);
    while request_count(&log) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    // The announcer polls every second: the server's `Reannounce-After` keeps it quiet.
    std::thread::sleep(Duration::from_secs(3));
    let requests = log.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(requests.len(), 1, "one announcement per interval");
    assert!(
        requests[0].body.contains("tcp://0.0.0.0:"),
        "announced: {}",
        requests[0].body
    );
    assert!(
        requests[0].body.contains(&format!(":{port}")),
        "{}",
        requests[0].body
    );
    drop(requests);
    assert!(
        daemon.stdout().contains("Global Discovery: announced"),
        "{}",
        daemon.stdout()
    );
}

/// A relay that answers the join only after a while: the daemon announces its listening
/// address first, then the relay address once it is joined.
#[test]
fn daemon_announces_again_when_its_relay_address_changes() {
    isolate_proxy_env();
    let home = temp_home("announce-relay");
    let (announce, log) = announce_server();
    let relay_identity = DeviceIdentity::from_seed(&[0x82; 32]).expect("relay identity");
    let relay_port = spawn_tls_service(&relay_identity, RELAY_ALPN, true, |mut stream| {
        // Answers the join late: the daemon's first announcement goes out before it.
        std::thread::sleep(Duration::from_millis(1500));
        if read_frame(&mut stream).is_ok() {
            let _ = stream.write_all(&response_frame(0, "success"));
            drain_frames(&mut stream);
        }
    });
    let relay = format!(
        "relay://127.0.0.1:{relay_port}/?id={}",
        relay_identity.device_id.to_syncthing_string()
    );
    save_peer_config(&home, &valid_device_id(113), &["dynamic"], &[announce], &[]);
    let mut config = syncthing_cli::config::load_config(&home).expect("load configuration");
    config.static_relays = vec![relay.clone()];
    syncthing_cli::config::save_config(&home, &config).expect("save configuration");

    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let daemon = Cli::new(&home).spawn_daemon(&["--listen", &listen, "--no-local-discovery"]);
    daemon.wait_for_log_line("Reachable through relay", Duration::from_secs(15));

    let deadline = Instant::now() + Duration::from_secs(10);
    while request_count(&log) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let requests = log.lock().unwrap_or_else(|p| p.into_inner());
    assert!(
        requests.len() >= 2,
        "the new relay address is announced: {} announcement(s)",
        requests.len()
    );
    assert!(
        !requests[0].body.contains("relay://"),
        "first: {}",
        requests[0].body
    );
    assert!(
        requests[1].body.contains("relay://127.0.0.1:"),
        "second: {}",
        requests[1].body
    );
}
