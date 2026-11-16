//! Scriptable fake Syncthing peer: a local TLS 1.3 + BEP/1.0 server driven by a test closure,
//! used to exercise `syncthing_core::session` end to end without a real Syncthing instance.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use sha2::{Digest, Sha256};
use syncthing_core::session::PeerHandshakeResult;
use syncthing_core::{
    connect_and_sync, read_bep_message, write_bep_message, BepBlockInfo, BepClusterConfig,
    BepFileInfo, BepFolder, BepHello, BepIndex, BepMessageType, BepRequest, BepResponse,
    BepVersionCounter, DeviceId, DeviceIdentity, FolderIndexState, FsSyncStorage, SessionParams,
    SyncControl, SyncEvent, SyncTuning, BEP_ALPN,
};

/// Behaviour of one secondary connection: the connection and its index in accept order.
pub type SecondaryScript = Arc<dyn Fn(&mut PeerConn, usize) + Send + Sync>;

/// Read timeout of a single poll on the peer side.
const POLL: Duration = Duration::from_millis(50);

/// A message received from the device under test.
pub type Received = (BepMessageType, Vec<u8>);

/// A running fake peer. `join` returns every message the device under test sent to it.
pub struct FakePeer {
    pub addr: SocketAddr,
    pub device_id: DeviceId,
    /// Connections accepted after the primary one (secondary connections), at any time.
    pub accepted: Arc<AtomicUsize>,
    handle: JoinHandle<Vec<Received>>,
    secondary_log: Arc<Mutex<Vec<Vec<Received>>>>,
}

impl FakePeer {
    /// Starts a peer with the identity derived from `seed`, accepting one connection and
    /// running `script` on it after the TLS handshake and the `Hello` exchange. The peer
    /// announces a single connection in its `Hello`.
    pub fn spawn<F>(seed: u8, script: F) -> Self
    where
        F: FnOnce(&mut PeerConn) + Send + 'static,
    {
        Self::start(seed, 1, script, None)
    }

    /// Like [`Self::spawn`], but the peer announces `connections` connections in its `Hello`
    /// and accepts every further TCP connection (secondary connections) until `script` ends.
    /// `secondary` runs on each of them, given its index in accept order.
    pub fn spawn_with_secondaries<F, S>(seed: u8, connections: i32, script: F, secondary: S) -> Self
    where
        F: FnOnce(&mut PeerConn) + Send + 'static,
        S: Fn(&mut PeerConn, usize) + Send + Sync + 'static,
    {
        Self::start(seed, connections, script, Some(Arc::new(secondary)))
    }

    fn start<F>(seed: u8, connections: i32, script: F, secondary: Option<SecondaryScript>) -> Self
    where
        F: FnOnce(&mut PeerConn) + Send + 'static,
    {
        let identity = Arc::new(DeviceIdentity::from_seed(&[seed; 32]).expect("peer identity"));
        let device_id = identity.device_id;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake peer");
        let addr = listener.local_addr().expect("fake peer address");
        let accepted = Arc::new(AtomicUsize::new(0));
        let secondary_log = Arc::new(Mutex::new(Vec::new()));
        let (accepted_for_thread, log_for_thread) =
            (Arc::clone(&accepted), Arc::clone(&secondary_log));
        let handle = thread::spawn(move || {
            let (tcp, _) = listener.accept().expect("accept");
            let stop = Arc::new(AtomicBool::new(false));
            let acceptor = secondary.map(|secondary| {
                listener
                    .set_nonblocking(true)
                    .expect("non-blocking listener");
                let worker = AcceptWorker {
                    listener,
                    identity: Arc::clone(&identity),
                    connections,
                    secondary,
                    stop: Arc::clone(&stop),
                    accepted: accepted_for_thread,
                    log: log_for_thread,
                };
                thread::spawn(move || worker.run())
            });
            let mut conn = PeerConn::accept(&identity, tcp, connections);
            script(&mut conn);
            // Closing the primary connection first lets the device end its session, whatever
            // the secondary scripts still do.
            let received = std::mem::take(&mut conn.received);
            drop(conn);
            stop.store(true, Ordering::SeqCst);
            if let Some(acceptor) = acceptor {
                acceptor.join().expect("secondary script panicked");
            }
            received
        });
        Self {
            addr,
            device_id,
            accepted,
            handle,
            secondary_log,
        }
    }

    /// Waits for the script to end and returns the messages received from the device.
    pub fn join(self) -> Vec<Received> {
        self.handle.join().expect("fake peer script panicked")
    }

    /// Like [`Self::join`], also returning the messages received on each secondary connection
    /// (in the order they ended).
    pub fn join_with_secondaries(self) -> (Vec<Received>, Vec<Vec<Received>>) {
        let primary = self.handle.join().expect("fake peer script panicked");
        let secondaries = std::mem::take(&mut *self.secondary_log.lock().unwrap());
        (primary, secondaries)
    }
}

/// Accepts the secondary connections of a [`FakePeer`] until the primary script ends.
struct AcceptWorker {
    listener: TcpListener,
    identity: Arc<DeviceIdentity>,
    connections: i32,
    secondary: SecondaryScript,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    log: Arc<Mutex<Vec<Vec<Received>>>>,
}

impl AcceptWorker {
    fn run(self) {
        let mut workers = Vec::new();
        while !self.stop.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((tcp, _)) => {
                    let index = self.accepted.fetch_add(1, Ordering::SeqCst);
                    let (identity, connections, secondary, log) = (
                        Arc::clone(&self.identity),
                        self.connections,
                        Arc::clone(&self.secondary),
                        Arc::clone(&self.log),
                    );
                    workers.push(thread::spawn(move || {
                        tcp.set_nonblocking(false)
                            .expect("blocking secondary stream");
                        let mut conn = PeerConn::accept(&identity, tcp, connections);
                        secondary(&mut conn, index);
                        log.lock().unwrap().push(conn.received);
                    }));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
        for worker in workers {
            worker.join().expect("secondary script panicked");
        }
    }
}

/// The peer side of one BEP connection.
pub struct PeerConn {
    stream: StreamOwned<ServerConnection, TcpStream>,
    device_id: DeviceId,
    /// Files served to block `Request`s, by `folder/name`.
    served: HashMap<String, Vec<u8>>,
    /// Every post-`Hello` message received so far, in order.
    pub received: Vec<Received>,
}

impl PeerConn {
    fn accept(identity: &DeviceIdentity, tcp: TcpStream, connections: i32) -> Self {
        let provider = Arc::new(rustls_rustcrypto::provider());
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(identity.cert_der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone())),
            )
            .expect("server certificate");
        config.alpn_protocols = vec![BEP_ALPN.to_vec()];
        let tls = ServerConnection::new(Arc::new(config)).expect("server connection");
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut stream = StreamOwned::new(tls, tcp);
        let hello = BepHello {
            device_name: "fake-peer".to_owned(),
            client_name: "syncthing".to_owned(),
            client_version: "v2.0.0-test".to_owned(),
            num_connections: connections,
            timestamp: 0,
        };
        hello.exchange(&mut stream).expect("Hello exchange");
        stream.get_mut().set_read_timeout(Some(POLL)).unwrap();
        Self {
            stream,
            device_id: identity.device_id,
            served: HashMap::new(),
            received: Vec::new(),
        }
    }

    /// Makes `content` available to block requests for `folder/name` and returns its metadata
    /// (single block, version `version` by this peer) for an `Index`.
    pub fn file(&mut self, folder: &str, name: &str, content: &[u8], version: u64) -> BepFileInfo {
        self.served
            .insert(format!("{folder}/{name}"), content.to_vec());
        BepFileInfo {
            name: name.to_owned(),
            is_dir: false,
            size: content.len() as i64,
            modified_s: 1_700_000_000 + version as i64,
            deleted: false,
            invalid: false,
            sequence: version as i64,
            versions: vec![BepVersionCounter {
                short_device_id: u64::from_be_bytes(self.device_id.0[..8].try_into().unwrap()),
                value: version,
            }],
            blocks: vec![BepBlockInfo {
                offset: 0,
                size: content.len() as i32,
                hash: Sha256::digest(content).to_vec(),
            }],
        }
    }

    pub fn send(&mut self, msg_type: BepMessageType, payload: &[u8]) {
        write_bep_message(&mut self.stream, msg_type, payload).expect("peer send");
    }

    /// Sends the secondary `ClusterConfig` that promotes a secondary connection.
    pub fn promote(&mut self) {
        self.send(
            BepMessageType::ClusterConfig,
            &BepClusterConfig::encode_secondary(),
        );
    }

    /// Sends a block `Request` for `folder/name` and waits up to `timeout` for its `Response`
    /// (answering the device's own requests meanwhile). `None` if no `Response` came in time.
    pub fn request(
        &mut self,
        id: i32,
        folder: &str,
        name: &str,
        timeout: Duration,
    ) -> Option<BepResponse> {
        let req = BepRequest {
            id,
            folder: folder.to_owned(),
            name: name.to_owned(),
            offset: 0,
            size: 0,
            hash: Vec::new(),
        };
        self.send(BepMessageType::Request, &req.encode());
        let payload = self.wait_for(BepMessageType::Response, timeout)?;
        BepResponse::decode(&payload).ok()
    }

    /// Sends a `ClusterConfig` announcing `folders` as shared with `device`.
    pub fn send_cluster_config(&mut self, folders: &[&str], device: DeviceId) {
        let entries: Vec<(BepFolder, FolderIndexState)> = folders
            .iter()
            .map(|id| {
                (
                    BepFolder {
                        id: (*id).to_owned(),
                        label: (*id).to_owned(),
                        device_ids: vec![self.device_id, device],
                    },
                    FolderIndexState::default(),
                )
            })
            .collect();
        let payload =
            BepClusterConfig::encode_for_peers(&entries, self.device_id, "fake-peer", device);
        self.send(BepMessageType::ClusterConfig, &payload);
    }

    /// Sends an `Index` (or `IndexUpdate` when `update`) for `folder`.
    pub fn send_index(&mut self, folder: &str, files: Vec<BepFileInfo>, update: bool) {
        let msg_type = if update {
            BepMessageType::IndexUpdate
        } else {
            BepMessageType::Index
        };
        let index = BepIndex {
            folder: folder.to_owned(),
            files,
        };
        self.send(msg_type, &index.encode());
    }

    /// Reads one message if any arrives within the poll interval. Block `Request`s are recorded
    /// but not answered. Returns `Err` once the device closed the connection.
    fn poll(&mut self) -> io::Result<Option<Received>> {
        match read_bep_message(&mut self.stream) {
            Ok(msg) => {
                self.received.push(msg.clone());
                Ok(Some(msg))
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Answers a block `Request` from the served files.
    pub fn answer(&mut self, request_payload: &[u8]) {
        let req = BepRequest::decode(request_payload).expect("request");
        let resp = match self.served.get(&format!("{}/{}", req.folder, req.name)) {
            Some(content) => BepResponse {
                id: req.id,
                data: content.clone(),
                code: 0,
            },
            None => BepResponse {
                id: req.id,
                data: Vec::new(),
                code: 2,
            },
        };
        self.send(BepMessageType::Response, &resp.encode());
    }

    /// Waits up to `timeout` for a message of `wanted` type, answering block requests
    /// meanwhile. Returns its payload, or `None` on timeout / disconnection.
    pub fn wait_for(&mut self, wanted: BepMessageType, timeout: Duration) -> Option<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.poll() {
                Ok(Some((msg_type, payload))) => {
                    if msg_type == wanted {
                        return Some(payload);
                    }
                    if msg_type == BepMessageType::Request {
                        self.answer(&payload);
                    }
                }
                Ok(None) => {}
                Err(_) => return None,
            }
        }
        None
    }

    /// Serves block requests until the device has been silent for `idle` (or disconnected).
    pub fn serve_until_idle(&mut self, idle: Duration) {
        let mut last = Instant::now();
        while last.elapsed() < idle {
            match self.poll() {
                Ok(Some((msg_type, payload))) => {
                    last = Instant::now();
                    if msg_type == BepMessageType::Request {
                        self.answer(&payload);
                    }
                }
                Ok(None) => {}
                Err(_) => return,
            }
        }
    }
}

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
