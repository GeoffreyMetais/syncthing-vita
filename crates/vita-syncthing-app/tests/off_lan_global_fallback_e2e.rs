//! End-to-end test for the off-LAN Global Discovery fallback race:
//!
//! When a linked peer has a previously saved LAN IP in `devices.cfg` that is unreachable outside
//! the home LAN, `run_peer_session_loop` acquires a `SessionClaim` and blocks on the dead LAN
//! endpoint for `CONNECT_TIMEOUT` (4 s). While that dialing claim is held, the coordinator runs
//! `refresh_global_endpoints`.
//!
//! Before the fix, `refresh_global_endpoints` checked `claims.claimed_ids()`, mistook the
//! dialing claim for an established session, skipped Global Discovery, and left the app stuck
//! on `Peer unreachable (waiting for LAN)`.
//!
//! This test drives the real `run_peer_session_loop`, `refresh_global_endpoints_with`,
//! `SessionClaims`, and a real `syncthing-core` BEP/1.0 TLS server end to end and verifies:
//! 1. While `run_peer_session_loop` holds the claim on the unresponsive LAN endpoint,
//!    `refresh_global_endpoints_with` still queries Global Discovery and caches the peer's
//!    reachable endpoint.
//! 2. After the dead LAN attempt times out, `run_peer_session_loop` connects to the Global
//!    Discovery endpoint, completes TLS 1.3 + BEP/1.0 sync, and exchanges files in both
//!    directions.
//! 3. Once the session is established (`PeerConnected`), `claims.established_ids()` contains the
//!    peer and subsequent `refresh_global_endpoints_with` calls skip Global Discovery.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, BepFolder, BepHello, DeviceIdentity, FsSyncStorage,
    PeerEndpoint, SessionParams, SyncControl,
};
use vita_syncthing_app::domain::KnownDevice;
use vita_syncthing_app::infrastructure::peer_endpoints::{
    refresh_global_endpoints_with, run_peer_session_loop, PeerSessionShared, SharedDiscoveredAddrs,
    SharedGlobalEndpoints,
};
use vita_syncthing_app::infrastructure::session_guard::SessionClaims;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vita-syncthing-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn make_storage(root: &Path, folder_id: &str, peer_id: syncthing_core::DeviceId) -> FsSyncStorage {
    let mut storage = FsSyncStorage::new(root);
    storage.add_known_device(peer_id);
    storage.set_folder_path(folder_id, root);
    storage.set_folder_devices(folder_id, vec![peer_id]);
    storage.add_local_folder(BepFolder {
        id: folder_id.to_owned(),
        label: folder_id.to_owned(),
        device_ids: vec![peer_id],
        ..Default::default()
    });
    storage
}

#[test]
fn off_lan_stale_lan_ip_falls_back_to_global_discovery_and_syncs_both_ways() {
    let folder_id = "off-lan-e2e";
    let vita_dir = TempDir::new("vita-offlan");
    let server_dir = TempDir::new("server-offlan");

    let vita_identity = DeviceIdentity::from_seed(&[0xA1; 32]).unwrap();
    let server_identity = DeviceIdentity::from_seed(&[0xB2; 32]).unwrap();

    fs::write(vita_dir.0.join("from_vita.txt"), b"hello from vita off-lan").unwrap();
    fs::write(
        server_dir.0.join("from_server.txt"),
        b"hello from home server",
    )
    .unwrap();

    let vita_storage = make_storage(&vita_dir.0, folder_id, server_identity.device_id);
    let server_storage = make_storage(&server_dir.0, folder_id, vita_identity.device_id);

    // 1. Blackhole TCP listener simulating a stale/unresponsive LAN IP that keeps
    //    `connect_and_sync` blocked for `CONNECT_TIMEOUT` while holding `SessionClaim`.
    let dead_lan_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_lan_addr = dead_lan_listener.local_addr().unwrap();
    dead_lan_listener.set_nonblocking(true).unwrap();
    let dead_stop = Arc::new(AtomicBool::new(false));
    let dead_stop_bg = Arc::clone(&dead_stop);
    let dead_handle = thread::spawn(move || {
        let mut parked = Vec::new();
        while !dead_stop_bg.load(Ordering::Acquire) {
            match dead_lan_listener.accept() {
                Ok((stream, _)) => parked.push(stream),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
        drop(parked);
    });

    // 2. Real BEP/1.0 server listening on a separate endpoint (returned only by Global Discovery).
    let server_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_addr = server_listener.local_addr().unwrap();
    server_listener.set_nonblocking(true).unwrap();
    let server_control = Arc::new(SyncControl::new());
    let server_control_bg = Arc::clone(&server_control);
    let server_id_bg = server_identity.clone();
    let server_storage_bg = server_storage.clone();
    let vita_id = vita_identity.device_id;
    let server_handle = thread::spawn(move || {
        let hello = BepHello::new("home-server", "syncthing", "1.27.0");
        while !server_control_bg.is_stop_requested() {
            match server_listener.accept() {
                Ok((tcp, peer_addr)) => {
                    let incoming =
                        match accept_bep_tls_stream(&server_id_bg, tcp, Duration::from_secs(5)) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                    let params = SessionParams {
                        identity: &server_id_bg,
                        hello: &hello,
                        storage: &server_storage_bg,
                        expected_device_id: Some(vita_id),
                        timeout: Duration::from_secs(5),
                        control: Some(Arc::clone(&server_control_bg)),
                        secondary_inbox: None,
                    };
                    let _ = accept_and_sync(&params, incoming, peer_addr, |_| {});
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    });

    // 3. Vita peer session loop configured with the stale/unresponsive LAN address.
    let kd = KnownDevice {
        device_id: server_identity.device_id,
        name: "Home Server".to_owned(),
        address: dead_lan_addr.to_string(),
    };
    let discovered: SharedDiscoveredAddrs = Arc::new(Mutex::new(Vec::new()));
    let global_endpoints: SharedGlobalEndpoints = Arc::new(Mutex::new(Vec::new()));
    let claims = Arc::new(SessionClaims::new(4));
    let initial_lan_done = Arc::new(AtomicBool::new(true));
    let vita_control = Arc::new(SyncControl::new());
    let (tx, _rx) = mpsc::channel();

    let shared = PeerSessionShared {
        identity: vita_identity.clone(),
        hello: BepHello::new("ps-vita", "vita-syncthing", "0.1.0"),
        storage: vita_storage.clone(),
        discovered: Arc::clone(&discovered),
        global: Arc::clone(&global_endpoints),
        claims: Arc::clone(&claims),
        initial_lan_done: Arc::clone(&initial_lan_done),
        control: Arc::clone(&vita_control),
        tx: tx.clone(),
    };
    let kd_loop = kd.clone();
    let peer_handle = thread::spawn(move || run_peer_session_loop(kd_loop, shared));

    // 4. Wait until `run_peer_session_loop` is actively dialing the dead LAN IP (holding claim).
    let wait_claim_deadline = Instant::now() + Duration::from_secs(5);
    while !claims.is_claimed(&server_identity.device_id) {
        assert!(
            Instant::now() < wait_claim_deadline,
            "peer session loop never started dialing the configured LAN IP"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        claims.established_ids().is_empty(),
        "dialing claim must not be reported as established before PeerConnected"
    );

    // 5. Run Global Discovery refresh WHILE the dialing claim is held.
    let lookup_calls = Arc::new(AtomicUsize::new(0));
    let lookup_calls_ref = Arc::clone(&lookup_calls);
    let mut last_errors = Vec::new();
    refresh_global_endpoints_with(
        std::slice::from_ref(&kd),
        &global_endpoints,
        &claims,
        &vita_control,
        &tx,
        &mut last_errors,
        move |queried_id| {
            assert_eq!(queried_id, server_identity.device_id);
            lookup_calls_ref.fetch_add(1, Ordering::AcqRel);
            Ok(vec![PeerEndpoint::Direct(server_addr)])
        },
    );
    assert_eq!(
        lookup_calls.load(Ordering::Acquire),
        1,
        "Global Discovery lookup must run even while peer session loop holds a dialing claim"
    );

    // 6. Wait for the dead LAN attempt to time out, the fallback to `server_addr` to connect,
    //    both files to sync end-to-end, and the session to be marked established.
    let sync_deadline = Instant::now() + Duration::from_secs(20);
    let vita_got_server = vita_dir.0.join("from_server.txt");
    let server_got_vita = server_dir.0.join("from_vita.txt");
    loop {
        let both_synced = fs::read(&vita_got_server)
            .map(|b| b == b"hello from home server")
            .unwrap_or(false)
            && fs::read(&server_got_vita)
                .map(|b| b == b"hello from vita off-lan")
                .unwrap_or(false);
        let established = claims
            .established_ids()
            .iter()
            .any(|id| id.matches_peer(&server_identity.device_id));
        if both_synced && established {
            break;
        }
        assert!(
            Instant::now() < sync_deadline,
            "timed out waiting for off-LAN Global Discovery fallback sync \
             (vita_got_server={}, server_got_vita={}, established={})",
            vita_got_server.exists(),
            server_got_vita.exists(),
            established
        );
        thread::sleep(Duration::from_millis(50));
    }

    // 7. Once established, Global Discovery must NOT be queried again for this peer.
    let lookup_calls_after = Arc::clone(&lookup_calls);
    refresh_global_endpoints_with(
        std::slice::from_ref(&kd),
        &global_endpoints,
        &claims,
        &vita_control,
        &tx,
        &mut last_errors,
        move |_| {
            lookup_calls_after.fetch_add(1, Ordering::AcqRel);
            Ok(vec![PeerEndpoint::Direct(server_addr)])
        },
    );
    assert_eq!(
        lookup_calls.load(Ordering::Acquire),
        1,
        "Global Discovery lookup must be skipped once the session is established"
    );

    // Clean shutdown.
    vita_control.request_stop();
    server_control.request_stop();
    dead_stop.store(true, Ordering::Release);
    let _ = peer_handle.join();
    let _ = server_handle.join();
    let _ = dead_handle.join();
}
