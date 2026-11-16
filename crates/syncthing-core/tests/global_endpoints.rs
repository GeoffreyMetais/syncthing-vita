//! Endpoint URIs of Global Discovery (`discovery::global::resolve_endpoint_uri`) with host
//! names: a name is resolved, and the first address that is not unspecified is kept.

use syncthing_core::discovery::global::{resolve_endpoint_uri, PeerEndpoint};
use syncthing_core::DeviceId;

#[test]
fn a_host_name_is_resolved_to_a_usable_address() {
    // `localhost` resolves to loopback addresses, never to an unspecified one.
    match resolve_endpoint_uri("tcp://localhost:22000") {
        Some(PeerEndpoint::Direct(addr)) => {
            assert_eq!(addr.port(), 22000);
            assert!(addr.ip().is_loopback(), "{addr}");
        }
        other => panic!("expected a direct endpoint, got {other:?}"),
    }
}

#[test]
fn a_relay_host_name_is_resolved_with_its_device_id() {
    let id = DeviceId([0xCD; 32]);
    let uri = format!(
        "relay://localhost:22067/?id={}&pingInterval=1m0s",
        id.to_syncthing_string()
    );
    match resolve_endpoint_uri(&uri) {
        Some(PeerEndpoint::Relay { addr, device_id }) => {
            assert_eq!(addr.port(), 22067);
            assert!(addr.ip().is_loopback(), "{addr}");
            assert_eq!(device_id, id);
        }
        other => panic!("expected a relay endpoint, got {other:?}"),
    }
}

#[test]
fn an_unspecified_host_name_address_is_not_usable() {
    assert_eq!(resolve_endpoint_uri("tcp://0.0.0.0:22000"), None);
    assert_eq!(resolve_endpoint_uri("tcp://[::]:22000"), None);
}
