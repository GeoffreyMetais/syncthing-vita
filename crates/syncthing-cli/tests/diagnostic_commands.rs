//! The diagnostic commands `discover`, `lookup` and `relays`, run as the real binary. Global
//! Discovery and the relay pool are local HTTPS stand-ins; no Internet is reached.

mod common;

use std::net::UdpSocket;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use syncthing_cli::config::{load_config, save_config, DeviceConfig};
use syncthing_core::{Announce, DeviceIdentity, DISCOVERY_PORT};

use common::servers::{https_url, isolate_proxy_env, spawn_https_service, HttpReply};
use common::{free_port, temp_home, valid_device_id};

/// Runs `syncthing-cli --home <home> <args>` to completion, with the proxy variables removed.
fn run_cli(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_syncthing-cli"))
        .arg("--home")
        .arg(home)
        .args(args)
        .env_remove("HTTPS_PROXY")
        .env_remove("https_proxy")
        .env_remove("ALL_PROXY")
        .env_remove("all_proxy")
        .env_remove("SSL_CERT_FILE")
        .output()
        .expect("run syncthing-cli")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Writes a configuration whose only non-default fields are `edit`'s.
fn configure(home: &Path, edit: impl FnOnce(&mut syncthing_cli::config::Config)) {
    let mut config = load_config(home).expect("load configuration");
    edit(&mut config);
    save_config(home, &config).expect("save configuration");
}

fn peer(id: &str, name: &str) -> DeviceConfig {
    DeviceConfig {
        id: id.to_owned(),
        name: name.to_owned(),
        addresses: vec!["dynamic".to_owned()],
    }
}

/// An announcement of `id` at `addresses`, as another device would broadcast it.
fn announce_packet(id: &str, addresses: &[&str]) -> Vec<u8> {
    Announce {
        id: syncthing_core::DeviceId::parse_user_id(id).expect("valid device ID"),
        addresses: addresses.iter().map(|a| (*a).to_owned()).collect(),
        instance_id: 1,
    }
    .encode_packet()
}

/// Runs `discover --seconds <seconds>` while `packets` are sent to it on loopback, and returns
/// its output and how long it ran.
fn discover_while_sending(home: &Path, seconds: u64, packets: &[Vec<u8>]) -> (Output, Duration) {
    let started = Instant::now();
    let child = Command::new(env!("CARGO_BIN_EXE_syncthing-cli"))
        .arg("--home")
        .arg(home)
        .args(["discover", "--seconds", &seconds.to_string()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn discover");
    // The sender keeps announcing until the command ends (the command binds the port
    // after its start-up, so the first packets may come too early to be heard).
    let sender = UdpSocket::bind("127.0.0.1:0").expect("bind sender");
    let until = Instant::now() + Duration::from_secs(seconds) + Duration::from_millis(800);
    let target = format!("127.0.0.1:{DISCOVERY_PORT}");
    while Instant::now() < until {
        for packet in packets {
            let _ = sender.send_to(packet, &target);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let output = child.wait_with_output().expect("discover output");
    (output, started.elapsed())
}

#[test]
fn discover_runs_for_the_requested_time() {
    isolate_proxy_env();
    let home = temp_home("discover-empty");
    let started = Instant::now();
    let output = run_cli(&home, &["discover", "--seconds", "2"]);
    let elapsed = started.elapsed();

    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("Listening for LAN announcements for 2 s"),
        "stdout: {stdout}"
    );
    assert!(
        elapsed >= Duration::from_millis(1900),
        "discover ended after {elapsed:?}, before its 2 s listening period"
    );
}

#[test]
fn discover_names_configured_devices_and_lists_each_device_once() {
    isolate_proxy_env();
    let home = temp_home("discover-heard");
    let laptop = valid_device_id(81);
    let stranger = valid_device_id(82);
    configure(&home, |c| c.devices.push(peer(&laptop, "laptop")));

    let packets = [
        announce_packet(&laptop, &["tcp://127.0.0.1:22000"]),
        announce_packet(&stranger, &["tcp://127.0.0.1:22001"]),
    ];
    let (output, _) = discover_while_sending(&home, 3, &packets);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    let stdout = stdout_of(&output);

    let laptop_line = format!("{laptop}  tcp://127.0.0.1:22000  (configured as \"laptop\")");
    let stranger_line = format!("{stranger}  tcp://127.0.0.1:22001  (not configured)");
    assert!(stdout.contains(&laptop_line), "stdout: {stdout}");
    assert!(stdout.contains(&stranger_line), "stdout: {stdout}");
    // Repeated announcements are listed once.
    assert_eq!(stdout.matches(&laptop).count(), 1, "stdout: {stdout}");
    assert!(!stdout.contains("No device found."), "stdout: {stdout}");
}

/// A Global Discovery stand-in that knows `known` at two endpoints and no other device.
fn lookup_server(known: &str, relay_id: &str) -> (u16, DeviceIdentity) {
    let identity = DeviceIdentity::from_seed(&[0x61; 32]).expect("server identity");
    let known = known.to_owned();
    let relay_uri = format!("relay://127.0.0.1:22067/?id={relay_id}");
    let (port, _) = spawn_https_service(&identity, false, move |request| {
        if request.request_line.contains(&format!("device={known}")) {
            HttpReply::ok(&format!(
                "{{\"addresses\":[\"tcp://127.0.0.1:22000\",\"{relay_uri}\"]}}"
            ))
        } else {
            HttpReply::status("404 Not Found")
        }
    });
    (port, identity)
}

#[test]
fn lookup_lists_the_endpoints_a_server_announces() {
    isolate_proxy_env();
    let home = temp_home("lookup-found");
    let known = valid_device_id(83);
    let unknown = valid_device_id(84);
    let relay = valid_device_id(85);
    let (port, identity) = lookup_server(&known, &relay);
    configure(&home, |c| {
        c.global_lookup_servers = vec![https_url(port, &identity, "noannounce")];
    });

    let found = run_cli(&home, &["lookup", &known]);
    assert!(found.status.success(), "stderr: {}", stderr_of(&found));
    let stdout = stdout_of(&found);
    assert!(
        stdout.contains(&format!("127.0.0.1: {known}")),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  tcp://127.0.0.1:22000"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  relay://127.0.0.1:22067/?id={relay}")),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("is not announced"), "stdout: {stdout}");

    let missing = run_cli(&home, &["lookup", &unknown]);
    assert!(missing.status.success(), "stderr: {}", stderr_of(&missing));
    let stdout = stdout_of(&missing);
    assert!(
        stdout.contains(&format!("127.0.0.1: {unknown} is not announced")),
        "stdout: {stdout}"
    );
}

#[test]
fn lookup_fails_when_every_server_fails() {
    isolate_proxy_env();
    let home = temp_home("lookup-unreachable");
    let closed = format!("https://127.0.0.1:{}/v2/?noannounce", free_port());
    configure(&home, |c| c.global_lookup_servers = vec![closed]);

    let output = run_cli(&home, &["lookup", &valid_device_id(86)]);
    assert!(!output.status.success(), "lookup must fail");
    let stderr = stderr_of(&output);
    assert!(stderr.contains("lookup failed"), "stderr: {stderr}");
    assert!(
        stderr.contains("no Global Discovery server answered"),
        "stderr: {stderr}"
    );
}

#[test]
fn lookup_fails_when_every_server_url_is_invalid() {
    isolate_proxy_env();
    let home = temp_home("lookup-invalid-url");
    configure(&home, |c| {
        c.global_lookup_servers = vec!["not a url".to_owned()]
    });

    let output = run_cli(&home, &["lookup", &valid_device_id(87)]);
    assert!(!output.status.success(), "lookup must fail");
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("invalid lookup server URL"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("no Global Discovery server answered"),
        "stderr: {stderr}"
    );
}

#[test]
fn lookup_succeeds_when_one_of_two_servers_answers() {
    isolate_proxy_env();
    let home = temp_home("lookup-partial");
    let known = valid_device_id(88);
    let (port, identity) = lookup_server(&known, &valid_device_id(89));
    let closed = format!("https://127.0.0.1:{}/v2/?noannounce", free_port());
    configure(&home, |c| {
        c.global_lookup_servers = vec![closed, https_url(port, &identity, "noannounce")];
    });

    let output = run_cli(&home, &["lookup", &known]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    assert!(
        stdout_of(&output).contains("  tcp://127.0.0.1:22000"),
        "stdout: {}",
        stdout_of(&output)
    );
}

#[test]
fn relays_lists_the_relays_of_the_pool() {
    isolate_proxy_env();
    let home = temp_home("relays-pool");
    let relay = valid_device_id(90);
    let identity = DeviceIdentity::from_seed(&[0x63; 32]).expect("pool identity");
    let body = format!("{{\"relays\":[{{\"url\":\"relay://127.0.0.1:22067/?id={relay}\"}}]}}");
    let (port, _) = spawn_https_service(&identity, false, move |_| HttpReply::ok(&body));
    configure(&home, |c| {
        c.relay_pool_url = https_url(port, &identity, "pool")
    });

    let output = run_cli(&home, &["relays"]);
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    assert_eq!(
        stdout_of(&output).trim(),
        format!("relay://127.0.0.1:22067/?id={relay}")
    );
    assert!(
        stderr_of(&output).contains("1 relay(s) in 127.0.0.1"),
        "stderr: {}",
        stderr_of(&output)
    );
}
