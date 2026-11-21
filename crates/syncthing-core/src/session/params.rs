//! What an application provides to run a session with a peer.

use std::sync::Arc;
use std::time::Duration;

use super::control::SyncControl;
use super::secondary::SecondaryInbox;
use crate::bep::BepHello;
use crate::cert::DeviceIdentity;
use crate::device_id::DeviceId;
use crate::storage::FsSyncStorage;

/// Everything a session needs besides the peer's address. One value can be reused for every
/// connection attempt to a peer.
#[derive(Clone)]
pub struct SessionParams<'a> {
    /// Our certificate and private key.
    pub identity: &'a DeviceIdentity,
    /// The `Hello` we send: device name (also announced in `ClusterConfig`), client name and
    /// version, number of connections supported.
    pub hello: &'a BepHello,
    /// The shared folders: where they live and which devices each one is shared with.
    pub storage: &'a FsSyncStorage,
    /// The peer's `DeviceId`: the TLS handshake fails if its certificate does not match.
    /// `None` accepts any peer (its ID is reported in the result).
    pub expected_device_id: Option<DeviceId>,
    /// Timeout of the TCP connection, the TLS handshake and blocking socket operations.
    pub timeout: Duration,
    /// `Some`: the session stays connected until [`SyncControl::request_stop`]. `None`: it
    /// ends after one sync cycle.
    pub control: Option<Arc<SyncControl>>,
    /// `Some`: registers this session in the shared [`SecondaryInbox`] so inbound secondary
    /// connections (`ClusterConfig { secondary: true }`) opened by the peer can attach to this
    /// active primary session (`1+2` mode).
    pub secondary_inbox: Option<Arc<SecondaryInbox>>,
}
