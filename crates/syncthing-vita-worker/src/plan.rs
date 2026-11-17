//! What one cycle synchronizes, decided from the app's configuration with the app's own rules
//! (`spawn_sync_worker` and `AppState::sync_peers`):
//!
//! - every share is registered with its path, paused state, folder type (read-only shares are
//!   "Receive Only") and linked devices: `FsSyncStorage::set_folder_devices` is the
//!   authorization gate of every session, so a folder is only synchronized with its devices;
//! - a share whose path is the app's data directory or one of its parents is never
//!   registered (it would expose the identity seed);
//! - the peers are the known devices linked to at least one enabled share. Unlike the app,
//!   the worker never connects only to receive folder offers: accepting one needs the app.

use std::path::Path;

use syncthing_core::FsSyncStorage;

use crate::config::{is_protected_share_path, DeviceRecord, ShareRecord, WorkerConfig};
use crate::defaults::MAX_PEERS_PER_CYCLE;

/// Why a share is left out of the cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Its folder holds the app's data (identity seed, configuration).
    ProtectedPath,
    /// Paused in the app.
    Paused,
    /// No device is linked to it.
    NoDevice,
}

/// The storage and the peers of one cycle.
#[derive(Debug, Clone)]
pub struct SyncPlan {
    pub storage: FsSyncStorage,
    /// Devices to synchronize with, in `devices.cfg` order, at most `MAX_PEERS_PER_CYCLE`.
    pub peers: Vec<DeviceRecord>,
    /// Shares not synchronized by this cycle, with the reason.
    pub skipped: Vec<(String, SkipReason)>,
    /// Peers beyond `MAX_PEERS_PER_CYCLE`, left for the app.
    pub peers_left_out: usize,
}

impl SyncPlan {
    pub fn from_config(config: &WorkerConfig) -> Self {
        let mut storage = FsSyncStorage::new(config.sync_root());
        let mut skipped = Vec::new();
        let mut active: Vec<&ShareRecord> = Vec::new();
        for share in &config.shares {
            if is_protected_share_path(&config.data_dir, &share.target_path) {
                skipped.push((share.folder_id.clone(), SkipReason::ProtectedPath));
                continue;
            }
            register_share(&mut storage, share);
            if !share.enabled {
                skipped.push((share.folder_id.clone(), SkipReason::Paused));
            } else if share.devices.is_empty() {
                skipped.push((share.folder_id.clone(), SkipReason::NoDevice));
            } else {
                active.push(share);
            }
        }
        let eligible: Vec<DeviceRecord> = config
            .devices
            .iter()
            .filter(|d| active.iter().any(|s| s.is_linked_to(&d.device_id)))
            .cloned()
            .collect();
        let peers_left_out = eligible.len().saturating_sub(MAX_PEERS_PER_CYCLE);
        Self {
            storage,
            peers: eligible.into_iter().take(MAX_PEERS_PER_CYCLE).collect(),
            skipped,
            peers_left_out,
        }
    }

    /// `true` if the cycle has nothing to do.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

/// Registers `share` in `storage` exactly as the app's sync worker does.
fn register_share(storage: &mut FsSyncStorage, share: &ShareRecord) {
    storage.set_folder_path(&share.folder_id, Path::new(&share.target_path));
    storage.set_folder_paused(&share.folder_id, !share.enabled);
    storage.set_folder_type(&share.folder_id, share.folder_type());
    storage.set_folder_devices(&share.folder_id, share.devices.clone());
}
