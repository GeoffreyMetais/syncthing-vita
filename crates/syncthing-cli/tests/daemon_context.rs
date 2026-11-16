//! Shared daemon state: the stop-aware sleep, the "report once" memory and the per-device
//! table (session claims, LAN and Global Discovery addresses, relay URIs).

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use syncthing_cli::config::Config;
use syncthing_cli::daemon::context::{DaemonContext, DaemonOptions};
use syncthing_core::discovery::PeerEndpoint;
use syncthing_core::{DeviceId, DeviceIdentity, HttpsClient};

use common::temp_home;

fn device(seed: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[seed; 32])
        .expect("identity")
        .device_id
}

fn addr(text: &str) -> SocketAddr {
    text.parse().expect("socket address")
}

fn context(name: &str) -> Arc<DaemonContext> {
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
    let identity = DeviceIdentity::from_seed(&[1; 32]).expect("identity");
    Arc::new(DaemonContext::new(
        &temp_home(name),
        &Config::default(),
        identity,
        options,
    ))
}

#[test]
fn sleep_waits_the_whole_duration_when_no_stop_is_requested() {
    let ctx = context("sleep-full");
    let start = Instant::now();
    assert!(ctx.sleep(Duration::from_millis(300)));
    assert!(start.elapsed() >= Duration::from_millis(300));
    assert!(!ctx.is_stopping());
}

#[test]
fn sleep_returns_at_once_when_stopping() {
    let ctx = context("sleep-stopping");
    ctx.control.request_stop();
    assert!(ctx.is_stopping());
    let start = Instant::now();
    assert!(!ctx.sleep(Duration::from_secs(30)));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn sleep_gives_up_when_a_stop_arrives_meanwhile() {
    let ctx = context("sleep-interrupted");
    let stopper = Arc::clone(&ctx);
    let timer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        stopper.control.request_stop();
    });
    let start = Instant::now();
    assert!(!ctx.sleep(Duration::from_secs(30)));
    assert!(start.elapsed() < Duration::from_secs(5));
    timer.join().expect("timer thread");
}

#[test]
fn first_report_is_true_once_per_subject_and_detail() {
    let ctx = context("first-report");
    assert!(ctx.first_report("lookup:x", "timeout"));
    assert!(!ctx.first_report("lookup:x", "timeout"));
    // Another detail about the same subject is a new report.
    assert!(ctx.first_report("lookup:x", "refused"));
    assert!(ctx.first_report("other", "timeout"));
}

#[test]
fn claims_are_exclusive_and_released_one_device_at_a_time() {
    let ctx = context("claims");
    let (a, b) = (device(10), device(11));
    let mut table = ctx.table();
    assert!(!table.is_claimed(&a));
    assert!(table.claim(a));
    assert!(
        !table.claim(a),
        "a second session for the same device is refused"
    );
    assert!(table.claim(b));
    assert!(table.is_claimed(&a) && table.is_claimed(&b));

    table.release(a);
    assert!(!table.is_claimed(&a));
    assert!(table.is_claimed(&b), "releasing a leaves b claimed");
    assert!(table.claim(a), "a can be claimed again once released");
}

#[test]
fn lan_addresses_are_kept_per_device() {
    let ctx = context("lan-addrs");
    let (a, b, c) = (device(20), device(21), device(22));
    let mut table = ctx.table();
    assert!(table.lan_addrs(&a).is_empty());

    table.set_lan_addrs(a, vec![addr("192.168.1.10:22000")]);
    table.set_lan_addrs(b, vec![addr("192.168.1.11:22000")]);
    assert_eq!(table.lan_addrs(&a), vec![addr("192.168.1.10:22000")]);
    assert_eq!(table.lan_addrs(&b), vec![addr("192.168.1.11:22000")]);
    assert!(table.lan_addrs(&c).is_empty());

    // A new announcement replaces the previous addresses of the same device only.
    table.set_lan_addrs(a, vec![addr("10.0.0.2:22000"), addr("10.0.0.3:22000")]);
    assert_eq!(
        table.lan_addrs(&a),
        vec![addr("10.0.0.2:22000"), addr("10.0.0.3:22000")]
    );
    assert_eq!(table.lan_addrs(&b), vec![addr("192.168.1.11:22000")]);
}

#[test]
fn global_endpoints_are_kept_per_device() {
    let ctx = context("global-endpoints");
    let (a, b) = (device(30), device(31));
    let direct = PeerEndpoint::Direct(addr("203.0.113.10:22000"));
    let relay = PeerEndpoint::Relay {
        addr: addr("198.51.100.5:22067"),
        device_id: a,
    };
    let mut table = ctx.table();
    assert!(table.global_endpoints(&a).is_empty());

    table.set_global_endpoints(a, vec![direct.clone(), relay.clone()]);
    table.set_global_endpoints(b, vec![PeerEndpoint::Direct(addr("203.0.113.11:22000"))]);
    assert_eq!(table.global_endpoints(&a), vec![direct, relay]);
    assert_eq!(
        table.global_endpoints(&b),
        vec![PeerEndpoint::Direct(addr("203.0.113.11:22000"))]
    );

    table.set_global_endpoints(a, Vec::new());
    assert!(table.global_endpoints(&a).is_empty());
    assert_eq!(table.global_endpoints(&b).len(), 1);
}

#[test]
fn relay_uris_are_replaced_as_a_whole() {
    let ctx = context("relay-uris");
    let mut table = ctx.table();
    assert!(
        table.relay_uris().is_empty(),
        "no relay URI before one is set"
    );

    table.set_relay_uris(vec!["relay://198.51.100.5:22067/?id=AAA".into()]);
    assert_eq!(
        table.relay_uris(),
        vec!["relay://198.51.100.5:22067/?id=AAA".to_owned()]
    );
    table.set_relay_uris(Vec::new());
    assert!(table.relay_uris().is_empty());
}
