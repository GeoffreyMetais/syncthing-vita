//! Helpers driving end-to-end sessions of the device under test against a [`FakePeer`]:
//! identity, `Hello`, timings, session runner and decoding of what the peer received.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use syncthing_core::session::PeerHandshakeResult;
use syncthing_core::{
    connect_and_sync, BepClusterConfig, BepHello, BepIndex, BepMessageType, DeviceIdentity,
    FsSyncStorage, SessionParams, SyncControl, SyncEvent, SyncTuning,
};

use super::fake_peer::{FakePeer, Received};

/// Payloads of the received messages of type `wanted`.
pub fn payloads(received: &[Received], wanted: BepMessageType) -> Vec<&[u8]> {
    received
        .iter()
        .filter(|(t, _)| *t == wanted)
        .map(|(_, p)| p.as_slice())
        .collect()
}

/// Identity of the device under test (the PS Vita side) in end-to-end sessions.
pub fn device_under_test() -> DeviceIdentity {
    DeviceIdentity::from_seed(&[7u8; 32]).expect("device identity")
}

/// `Hello` of the device under test announcing `connections` connections.
pub fn hello_under_test(connections: i32) -> BepHello {
    let mut hello = BepHello::new("Test device", "syncthing-core-tests", "0.0.0");
    hello.num_connections = connections;
    hello
}

/// Timings of a persistent session with a rescan interval of `rescan` and a keepalive every
/// `keepalive`.
pub fn tuning(rescan: Duration, keepalive: Duration) -> SyncTuning {
    SyncTuning {
        rescan_interval: rescan,
        keepalive_interval: keepalive,
    }
}

/// A persistent session control with the given timings.
pub fn control_with(rescan: Duration, keepalive: Duration) -> Arc<SyncControl> {
    SyncControl::with_tuning(tuning(rescan, keepalive))
}

/// Runs a session of the device under test against `peer` and reports its events to
/// `on_event`. With a `control` the session is persistent: it ends when the peer disconnects
/// (its script ends) or when a stop is requested.
pub fn run_session(
    peer: &FakePeer,
    storage: &FsSyncStorage,
    hello: &BepHello,
    control: Option<Arc<SyncControl>>,
    on_event: impl FnMut(SyncEvent),
) -> io::Result<PeerHandshakeResult> {
    let identity = device_under_test();
    let params = SessionParams {
        identity: &identity,
        hello,
        storage,
        expected_device_id: Some(peer.device_id),
        timeout: Duration::from_secs(4),
        control,
        secondary_inbox: None,
    };
    connect_and_sync(&params, peer.addr, on_event)
}

/// Payloads of the `Index` and `IndexUpdate` messages of `received`, in order.
pub fn index_messages(received: &[Received]) -> Vec<BepIndex> {
    received
        .iter()
        .filter(|(t, _)| matches!(t, BepMessageType::Index | BepMessageType::IndexUpdate))
        .map(|(_, p)| BepIndex::decode(p).expect("index"))
        .collect()
}

/// Folder IDs announced by a `ClusterConfig` payload.
pub fn announced_folders(payload: &[u8]) -> Vec<String> {
    BepClusterConfig::decode(payload)
        .expect("ClusterConfig")
        .folders
        .into_iter()
        .map(|f| f.id)
        .collect()
}
