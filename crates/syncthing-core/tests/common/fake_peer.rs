//! Scriptable fake Syncthing peer: a local TLS 1.3 + BEP/1.0 server driven by a test closure,
//! used to exercise `syncthing_core::session` end to end without a real Syncthing instance.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use sha2::{Digest, Sha256};
use syncthing_core::{
    read_bep_message, write_bep_message, BepBlockInfo, BepClusterConfig, BepFileInfo, BepFolder,
    BepHello, BepIndex, BepMessageType, BepRequest, BepResponse, BepVersionCounter, DeviceId,
    FolderIndexState, VitaIdentity, BEP_ALPN,
};

/// Read timeout of a single poll on the peer side.
const POLL: Duration = Duration::from_millis(50);

/// A message received from the device under test.
pub type Received = (BepMessageType, Vec<u8>);

/// A running fake peer. `join` returns every message the device under test sent to it.
pub struct FakePeer {
    pub addr: SocketAddr,
    pub device_id: DeviceId,
    handle: JoinHandle<Vec<Received>>,
}

impl FakePeer {
    /// Starts a peer with the identity derived from `seed`, accepting one connection and
    /// running `script` on it after the TLS handshake and the `Hello` exchange.
    pub fn spawn<F>(seed: u8, script: F) -> Self
    where
        F: FnOnce(&mut PeerConn) + Send + 'static,
    {
        let identity = VitaIdentity::from_seed(&[seed; 32]).expect("peer identity");
        let device_id = identity.device_id;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake peer");
        let addr = listener.local_addr().expect("fake peer address");
        let handle = std::thread::spawn(move || {
            let (tcp, _) = listener.accept().expect("accept");
            let mut conn = PeerConn::accept(&identity, tcp);
            script(&mut conn);
            conn.received
        });
        Self {
            addr,
            device_id,
            handle,
        }
    }

    /// Waits for the script to end and returns the messages received from the device.
    pub fn join(self) -> Vec<Received> {
        self.handle.join().expect("fake peer script panicked")
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
    fn accept(identity: &VitaIdentity, tcp: TcpStream) -> Self {
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
            num_connections: 1,
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
        let payload = BepClusterConfig::encode_for_peers(&entries, self.device_id, device);
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
