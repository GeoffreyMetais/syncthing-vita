//! What the daemon's threads share: identity, folders, configured devices, the addresses
//! learnt by discovery and which devices are connected.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use syncthing_core::discovery::PeerEndpoint;
use syncthing_core::{
    BepFolder, BepHello, DeviceId, DeviceIdentity, FolderType, FsSyncStorage, HttpsClient,
    SyncControl, SyncTuning,
};

use super::activity::SyncActivity;
use super::timing::{KEEPALIVE_INTERVAL, STOP_CHECK_STEP};
use crate::config::Config;
use crate::paths::db_dir;

/// Client name sent in our `Hello`.
pub const CLIENT_NAME: &str = "syncthing-cli";

/// Which network features run (configuration, overridden by `run` flags).
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    /// `None`: do not listen (outgoing connections only).
    pub listen: Option<SocketAddr>,
    pub local_discovery: bool,
    pub global_discovery: bool,
    pub relays: bool,
    pub announce_servers: Vec<String>,
    pub lookup_servers: Vec<String>,
    pub relay_pool_url: String,
    pub static_relays: Vec<String>,
    /// How Global Discovery and the relay pool are reached (proxy, extra roots).
    pub https: HttpsClient,
}

/// A configured remote device.
#[derive(Debug, Clone)]
pub struct KnownPeer {
    pub id: DeviceId,
    pub name: String,
    /// Configured addresses (`tcp://host:port`, `host:port`; `dynamic` is skipped).
    pub addresses: Vec<String>,
}

/// Mutable state shared by the threads.
#[derive(Default)]
pub struct PeerTable {
    /// Devices with a session established or being established (one session per device).
    claimed: Vec<DeviceId>,
    /// Addresses announced on the LAN.
    lan: Vec<(DeviceId, Vec<SocketAddr>)>,
    /// Endpoints found on Global Discovery.
    global: Vec<(DeviceId, Vec<PeerEndpoint>)>,
    /// `relay://` URIs we are reachable through.
    relay_uris: Vec<String>,
    /// Things already reported once (unknown devices, folder offers).
    reported: HashSet<String>,
    pub activity: SyncActivity,
}

pub struct DaemonContext {
    pub identity: DeviceIdentity,
    pub hello: BepHello,
    pub storage: FsSyncStorage,
    pub control: Arc<SyncControl>,
    pub peers: Vec<KnownPeer>,
    pub options: DaemonOptions,
    /// Port we accept connections on, once listening.
    pub listen_port: Mutex<Option<u16>>,
    table: Mutex<PeerTable>,
}

impl DaemonContext {
    pub fn new(
        home: &Path,
        config: &Config,
        identity: DeviceIdentity,
        options: DaemonOptions,
    ) -> Self {
        let peers = config
            .devices
            .iter()
            .filter_map(|d| {
                Some(KnownPeer {
                    id: d.device_id()?,
                    name: d.name.clone(),
                    addresses: d.addresses.clone(),
                })
            })
            .collect();
        let control = SyncControl::with_tuning(SyncTuning {
            rescan_interval: Duration::from_secs(config.rescan_interval_secs.max(1)),
            keepalive_interval: KEEPALIVE_INTERVAL,
        });
        Self {
            hello: BepHello::new(&config.device_name, CLIENT_NAME, env!("CARGO_PKG_VERSION")),
            storage: build_storage(home, config),
            identity,
            control,
            peers,
            options,
            listen_port: Mutex::new(None),
            table: Mutex::new(PeerTable::default()),
        }
    }

    pub fn table(&self) -> MutexGuard<'_, PeerTable> {
        self.table.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn is_stopping(&self) -> bool {
        self.control.is_stop_requested()
    }

    /// Sleeps up to `duration`; `false` if a stop was requested meanwhile.
    pub fn sleep(&self, duration: Duration) -> bool {
        let mut left = duration;
        while !left.is_zero() {
            if self.is_stopping() {
                return false;
            }
            let step = left.min(STOP_CHECK_STEP);
            std::thread::sleep(step);
            left -= step;
        }
        !self.is_stopping()
    }

    pub fn peer(&self, id: &DeviceId) -> Option<&KnownPeer> {
        self.peers.iter().find(|p| p.id == *id)
    }

    /// `true` the first time `key` is reported (to log a recurring situation once).
    pub fn first_report(&self, key: String) -> bool {
        self.table().reported.insert(key)
    }
}

impl PeerTable {
    /// Reserves the session slot of `id`; `false` if a session already holds it.
    pub fn claim(&mut self, id: DeviceId) -> bool {
        if self.claimed.contains(&id) {
            return false;
        }
        self.claimed.push(id);
        true
    }

    pub fn release(&mut self, id: DeviceId) {
        self.claimed.retain(|c| *c != id);
    }

    pub fn is_claimed(&self, id: &DeviceId) -> bool {
        self.claimed.contains(id)
    }

    pub fn set_lan_addrs(&mut self, id: DeviceId, addrs: Vec<SocketAddr>) {
        match self.lan.iter_mut().find(|(d, _)| *d == id) {
            Some(slot) => slot.1 = addrs,
            None => self.lan.push((id, addrs)),
        }
    }

    pub fn lan_addrs(&self, id: &DeviceId) -> Vec<SocketAddr> {
        self.lan
            .iter()
            .find(|(d, _)| d == id)
            .map(|(_, a)| a.clone())
            .unwrap_or_default()
    }

    pub fn set_global_endpoints(&mut self, id: DeviceId, endpoints: Vec<PeerEndpoint>) {
        match self.global.iter_mut().find(|(d, _)| *d == id) {
            Some(slot) => slot.1 = endpoints,
            None => self.global.push((id, endpoints)),
        }
    }

    pub fn global_endpoints(&self, id: &DeviceId) -> Vec<PeerEndpoint> {
        self.global
            .iter()
            .find(|(d, _)| d == id)
            .map(|(_, e)| e.clone())
            .unwrap_or_default()
    }

    pub fn set_relay_uris(&mut self, uris: Vec<String>) {
        self.relay_uris = uris;
    }

    pub fn relay_uris(&self) -> Vec<String> {
        self.relay_uris.clone()
    }
}

/// Declares the configured folders: where they live, who they are shared with (the only
/// devices they are ever synchronized with), their type, and that we announce them.
fn build_storage(home: &Path, config: &Config) -> FsSyncStorage {
    let mut storage = FsSyncStorage::new(db_dir(home));
    for folder in &config.folders {
        let devices = folder.device_ids();
        storage.set_folder_path(&folder.id, &folder.path);
        storage.set_folder_devices(&folder.id, devices.clone());
        storage.set_folder_paused(&folder.id, folder.paused);
        storage.set_folder_type(
            &folder.id,
            if folder.receive_only {
                FolderType::ReceiveOnly
            } else {
                FolderType::SendReceive
            },
        );
        if !devices.is_empty() {
            storage.add_local_folder(BepFolder {
                id: folder.id.clone(),
                label: folder.label.clone(),
                device_ids: devices,
            });
        }
    }
    storage
}
