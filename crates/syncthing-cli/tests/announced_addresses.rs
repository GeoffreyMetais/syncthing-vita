//! Global Discovery announcements: the direct address (once listening) and the relay URIs.

mod common;

use syncthing_cli::config::Config;
use syncthing_cli::daemon::context::{DaemonContext, DaemonOptions};
use syncthing_cli::daemon::global::announced_addresses;
use syncthing_core::{DeviceIdentity, HttpsClient};

use common::temp_home;

fn context() -> DaemonContext {
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
        &temp_home("announced-addresses"),
        &Config::default(),
        DeviceIdentity::from_seed(&[2; 32]).expect("identity"),
        options,
    )
}

#[test]
fn nothing_is_announced_before_listening_or_relaying() {
    let ctx = context();
    assert!(announced_addresses(&ctx).is_empty());
}

#[test]
fn listening_port_is_announced_as_unspecified_address() {
    let ctx = context();
    *ctx.listen_port.lock().expect("listen port lock") = Some(22000);
    assert_eq!(
        announced_addresses(&ctx),
        vec!["tcp://0.0.0.0:22000".to_owned()]
    );
}

#[test]
fn relay_uris_follow_the_direct_address() {
    let ctx = context();
    *ctx.listen_port.lock().expect("listen port lock") = Some(22001);
    ctx.table()
        .set_relay_uris(vec!["relay://198.51.100.5:22067/?id=AAA".into()]);
    assert_eq!(
        announced_addresses(&ctx),
        vec![
            "tcp://0.0.0.0:22001".to_owned(),
            "relay://198.51.100.5:22067/?id=AAA".to_owned(),
        ]
    );
}

#[test]
fn relay_uris_alone_are_announced_without_a_listener() {
    let ctx = context();
    ctx.table()
        .set_relay_uris(vec!["relay://198.51.100.5:22067/?id=BBB".into()]);
    assert_eq!(
        announced_addresses(&ctx),
        vec!["relay://198.51.100.5:22067/?id=BBB".to_owned()]
    );
}
