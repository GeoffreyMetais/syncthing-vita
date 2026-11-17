//! Shared helpers: temp directories, the app's configuration written by the app's own
//! repository, and a `syncthing-core` peer listening on `127.0.0.1`.
#![allow(dead_code)]

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, BepFolder, BepHello, CycleDrain, DeviceId,
    DeviceIdentity, FsSyncStorage, SessionParams, SyncControl, SyncTuning,
};
use syncthing_vita_worker::CycleOptions;
use vita_syncthing_app::domain::{IgnoredFolder, KnownDevice, ShareConfigEntry, ShareMode};
use vita_syncthing_app::infrastructure::ConfigRepository;

/// Seed of the device under test (the PS Vita).
pub const VITA_SEED: [u8; 32] = [0x56; 32];
const PEER_TIMEOUT: Duration = Duration::from_secs(4);

/// A unique, empty directory for one test.
pub fn fresh_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("syncthing-vita-worker-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn vita_identity() -> DeviceIdentity {
    DeviceIdentity::from_seed(&VITA_SEED).unwrap()
}

/// The app's repository on `data_dir`.
pub fn repo(data_dir: &Path) -> ConfigRepository {
    ConfigRepository::new(data_dir.to_str().unwrap())
}

/// The app's data directory with an identity seed and `plugin.cfg` (`enabled`).
pub fn data_dir_with_identity(name: &str, enabled: bool) -> PathBuf {
    let dir = fresh_temp_dir(name);
    std::fs::write(dir.join("device_seed.bin"), VITA_SEED).unwrap();
    std::fs::write(
        dir.join("plugin.cfg"),
        format!("enabled={}\n", u8::from(enabled)),
    )
    .unwrap();
    dir
}

pub fn device(id: DeviceId, name: &str, address: &str) -> KnownDevice {
    KnownDevice {
        device_id: id,
        name: name.to_owned(),
        address: address.to_owned(),
    }
}

pub fn share(folder: &str, path: &Path, enabled: bool, devices: Vec<DeviceId>) -> ShareConfigEntry {
    ShareConfigEntry {
        folder_id: folder.to_owned(),
        label: format!("{folder} label"),
        target_path: path.to_str().unwrap().to_owned(),
        enabled,
        mode: ShareMode::ReadWrite,
        shared_devices: devices,
    }
}

pub fn ignored(folder: &str, device_id: DeviceId) -> IgnoredFolder {
    IgnoredFolder {
        folder_id: folder.to_owned(),
        device_id,
    }
}

/// Options of a test cycle: no discovery (no network), short drain and deadline.
pub fn test_options() -> CycleOptions {
    CycleOptions {
        lan_discovery: false,
        global_discovery: false,
        drain: CycleDrain {
            idle: Duration::from_millis(1500),
            max: Duration::from_secs(10),
        },
        max_duration: Duration::from_secs(60),
        keepalive_interval: Duration::from_secs(20),
    }
}

/// A `syncthing-core` device listening on `127.0.0.1` for one session from the PS Vita,
/// sharing `folders` (each at `<root>/<folder>`) with it.
pub struct ListeningPeer {
    pub addr: SocketAddr,
    pub device_id: DeviceId,
    pub root: PathBuf,
    handle: JoinHandle<()>,
}

impl ListeningPeer {
    pub fn spawn(seed: u8, root: &Path, folders: &[&str]) -> Self {
        let identity = DeviceIdentity::from_seed(&[seed; 32]).unwrap();
        let device_id = identity.device_id;
        let vita = vita_identity().device_id;
        let mut storage = FsSyncStorage::new(root.join("db"));
        for folder in folders {
            let dir = root.join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            storage.set_folder_path(folder, &dir);
            storage.set_folder_devices(folder, vec![vita]);
            storage.add_local_folder(BepFolder {
                id: (*folder).to_owned(),
                label: (*folder).to_owned(),
                device_ids: vec![vita],
            });
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let Ok((tcp, peer_addr)) = listener.accept() else {
                return;
            };
            let Ok(incoming) = accept_bep_tls_stream(&identity, tcp, PEER_TIMEOUT) else {
                return;
            };
            let hello = BepHello::new("peer", "syncthing-core-tests", "1");
            // Persistent: the session lasts until the PS Vita closes it (end of its cycle).
            let control: Arc<SyncControl> = SyncControl::with_tuning(SyncTuning {
                rescan_interval: Duration::from_secs(3600),
                keepalive_interval: Duration::from_secs(20),
            });
            let params = SessionParams {
                identity: &identity,
                hello: &hello,
                storage: &storage,
                expected_device_id: Some(vita),
                timeout: PEER_TIMEOUT,
                control: Some(control),
            };
            let _ = accept_and_sync(&params, incoming, peer_addr, |_| {});
        });
        Self {
            addr,
            device_id,
            root: root.to_path_buf(),
            handle,
        }
    }

    /// Waits for the session to end.
    pub fn join(self) {
        self.handle.join().unwrap();
    }
}
