//! Peers whose session is in its initial phase (`ClusterConfig` / `Index` exchange, local scan
//! and initial download), between `PeerConnected` and their first `SyncCycleCompleted`.

use syncthing_core::DeviceId;

/// Set of peers currently in their initial sync phase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitialSyncPeers {
    peers: Vec<DeviceId>,
}

impl InitialSyncPeers {
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
