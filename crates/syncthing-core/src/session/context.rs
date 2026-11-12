//! Per-session context shared by every phase of a BEP session: who we are, who the peer is,
//! where files live, transfer statistics and the event sink.

use super::events::SyncEvent;
use crate::cert::VitaIdentity;
use crate::device_id::DeviceId;
use crate::storage::FsSyncStorage;

/// Cumulative transfer statistics of one peer session.
#[derive(Debug, Default, Clone, Copy)]
pub struct SyncStats {
    pub files_up_to_date: usize,
    pub files_downloaded: usize,
    pub bytes_downloaded: usize,
    pub blocks_uploaded: usize,
    pub bytes_uploaded: usize,
}

impl SyncStats {
    /// Builds the `SyncCycleCompleted` event reporting these statistics for `peer_device_id`.
    pub fn cycle_completed(
        &self,
        peer_device_id: DeviceId,
        active_connections: usize,
    ) -> SyncEvent {
        SyncEvent::SyncCycleCompleted {
            peer_device_id,
            active_connections,
            files_up_to_date: self.files_up_to_date,
            files_downloaded: self.files_downloaded,
            bytes_downloaded: self.bytes_downloaded,
            blocks_uploaded: self.blocks_uploaded,
            bytes_uploaded: self.bytes_uploaded,
        }
    }
}

/// Mutable state and dependencies shared by the session phases (never the TLS streams, which
/// are passed separately so they can be borrowed alongside the context).
pub struct SessionContext<'a> {
    pub identity: &'a VitaIdentity,
    pub storage: &'a FsSyncStorage,
    pub peer_device_id: DeviceId,
    pub stats: SyncStats,
    next_request_id: i32,
    on_event: &'a mut dyn FnMut(SyncEvent),
}

impl<'a> SessionContext<'a> {
    pub fn new(
        identity: &'a VitaIdentity,
        storage: &'a FsSyncStorage,
        peer_device_id: DeviceId,
        on_event: &'a mut dyn FnMut(SyncEvent),
    ) -> Self {
        Self {
            identity,
            storage,
            peer_device_id,
            stats: SyncStats::default(),
            next_request_id: 1,
            on_event,
        }
    }

    /// Our own `DeviceId`.
    pub fn own_id(&self) -> DeviceId {
        self.identity.device_id
    }

    /// Forwards `event` to the session's event sink.
    pub fn emit(&mut self, event: SyncEvent) {
        (self.on_event)(event);
    }

    /// Emits a `SyncCycleCompleted` event with the current statistics.
    pub fn emit_cycle_completed(&mut self, active_connections: usize) {
        let event = self
            .stats
            .cycle_completed(self.peer_device_id, active_connections);
        self.emit(event);
    }

    /// Allocates the next non-zero BEP `Request` id.
    pub fn next_request_id(&mut self) -> i32 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        id
    }
}
