//! Per-peer session state, so one peer disconnecting never hides (or fakes) the state of the
//! others. Initial sync: peers whose session is in its initial phase (`ClusterConfig` / `Index` exchange, local scan
//! and initial download), between `PeerConnected` and their first `SyncCycleCompleted`.

use syncthing_core::DeviceId;

/// A set of peers in a given session state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerSet {
    peers: Vec<DeviceId>,
}

/// Peers currently in their initial sync phase.
pub type InitialSyncPeers = PeerSet;

/// Peers whose persistent session is established (first sync cycle completed, not ended):
/// `begin` on `SyncCycleCompleted`, `end` on `SessionEnded`.
pub type ConnectedPeers = PeerSet;

impl PeerSet {
    /// A session with `peer` was established: its initial phase starts.
    pub fn begin(&mut self, peer: DeviceId) {
        if !self.peers.contains(&peer) {
            self.peers.push(peer);
        }
    }

    /// The initial phase with `peer` completed, or its session ended.
    pub fn end(&mut self, peer: DeviceId) {
        self.peers.retain(|p| *p != peer);
    }

    /// The sync worker stopped: no session remains.
    pub fn clear(&mut self) {
        self.peers.clear();
    }

    /// True while at least one peer is in its initial phase.
    pub fn is_active(&self) -> bool {
        !self.peers.is_empty()
    }
}
