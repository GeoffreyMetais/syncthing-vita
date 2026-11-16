//! Dialer helpers: resolving configured addresses, the retry jitter, the ordered list of
//! endpoints tried for a device, and the labels used in the logs.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use syncthing_cli::config::Config;
use syncthing_cli::daemon::context::{DaemonContext, DaemonOptions, KnownPeer};
use syncthing_cli::daemon::dialer::{jitter, label, resolve_configured, targets};
use syncthing_cli::daemon::timing::RETRY_JITTER;
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

fn context(relays: bool) -> DaemonContext {
    let options = DaemonOptions {
        listen: None,
        local_discovery: false,
        global_discovery: false,
        relays,
        announce_servers: Vec::new(),
        lookup_servers: Vec::new(),
        relay_pool_url: String::new(),
        static_relays: Vec::new(),
        https: HttpsClient::default(),
    };
    DaemonContext::new(
        &temp_home("dialer-targets"),
        &Config::default(),
        DeviceIdentity::from_seed(&[1; 32]).expect("identity"),
        options,
    )
}

#[test]
fn configured_addresses_resolve_with_or_without_scheme() {
    assert_eq!(
        resolve_configured("tcp://127.0.0.1:22000"),
        vec![addr("127.0.0.1:22000")]
    );
    assert_eq!(
        resolve_configured("127.0.0.1:22000"),
        vec![addr("127.0.0.1:22000")]
    );
    assert_eq!(
        resolve_configured("  tcp://[::1]:22000  "),
        vec![addr("[::1]:22000")]
    );
    assert_eq!(
        resolve_configured("tcp://127.0.0.1:22000/extra"),
        vec![addr("127.0.0.1:22000")]
    );
}

#[test]
fn dynamic_empty_and_unspecified_addresses_resolve_to_nothing() {
    assert!(resolve_configured("dynamic").is_empty());
    assert!(resolve_configured("DYNAMIC").is_empty());
    assert!(resolve_configured("").is_empty());
    assert!(resolve_configured("tcp://").is_empty());
    assert!(resolve_configured("0.0.0.0:22000").is_empty());
    assert!(resolve_configured("tcp://[::]:22000").is_empty());
    assert!(resolve_configured("no-port-here").is_empty());
}

#[test]
fn jitter_stays_below_the_retry_jitter() {
    let mut seen_nonzero = false;
    for _ in 0..50 {
        let value = jitter();
        assert!(value < RETRY_JITTER, "{value:?} >= {RETRY_JITTER:?}");
        seen_nonzero |= value > Duration::ZERO;
        std::thread::sleep(Duration::from_micros(1500));
    }
    assert!(seen_nonzero, "jitter is always zero");
}

#[test]
fn labels_show_the_scheme_of_each_endpoint() {
    assert_eq!(
        label(&PeerEndpoint::Direct(addr("127.0.0.1:22000"))),
        "tcp://127.0.0.1:22000"
    );
    let relay = PeerEndpoint::Relay {
        addr: addr("198.51.100.5:22067"),
        device_id: device(40),
    };
    assert_eq!(label(&relay), "relay://198.51.100.5:22067");
}

fn peer(addresses: &[&str]) -> KnownPeer {
    KnownPeer {
        id: device(41),
        name: "laptop".into(),
        addresses: addresses.iter().map(|a| (*a).to_owned()).collect(),
    }
}

#[test]
fn targets_list_configured_then_lan_then_global_addresses() {
    let ctx = context(false);
    let peer = peer(&["tcp://127.0.0.1:22000", "dynamic", "10.0.0.1:22000"]);
    {
        let mut table = ctx.table();
        table.set_lan_addrs(
            peer.id,
            vec![addr("192.168.1.5:22000"), addr("127.0.0.1:22000")],
        );
        table.set_global_endpoints(
            peer.id,
            vec![PeerEndpoint::Direct(addr("203.0.113.9:22000"))],
        );
    }
    let found = targets(&ctx, &peer);
    assert_eq!(
        found,
        vec![
            PeerEndpoint::Direct(addr("127.0.0.1:22000")),
            PeerEndpoint::Direct(addr("10.0.0.1:22000")),
            PeerEndpoint::Direct(addr("192.168.1.5:22000")),
            PeerEndpoint::Direct(addr("203.0.113.9:22000")),
        ]
    );
}

#[test]
fn relays_are_only_tried_last_and_only_when_enabled() {
    let relay = PeerEndpoint::Relay {
        addr: addr("198.51.100.5:22067"),
        device_id: device(41),
    };
    let peer = peer(&["tcp://127.0.0.1:22000"]);

    let disabled = context(false);
    disabled
        .table()
        .set_global_endpoints(peer.id, vec![relay.clone()]);
    assert_eq!(
        targets(&disabled, &peer),
        vec![PeerEndpoint::Direct(addr("127.0.0.1:22000"))],
        "relays are not dialed when disabled"
    );

    let enabled = context(true);
    enabled.table().set_global_endpoints(
        peer.id,
        vec![
            relay.clone(),
            PeerEndpoint::Direct(addr("203.0.113.9:22000")),
        ],
    );
    assert_eq!(
        targets(&enabled, &peer),
        vec![
            PeerEndpoint::Direct(addr("127.0.0.1:22000")),
            PeerEndpoint::Direct(addr("203.0.113.9:22000")),
            relay,
        ]
    );
}

#[test]
fn a_device_without_any_address_has_no_target() {
    let ctx = context(true);
    assert!(targets(&ctx, &peer(&["dynamic"])).is_empty());
}
