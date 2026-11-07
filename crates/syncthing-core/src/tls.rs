//! TLS 1.3 layer, X.509 identity, and bidirectional BEP/1.0 synchronization engine.
//! Uses pure Rust primitives (`rustls` + `rustls-rustcrypto` + `p256` + `sha2`)
//! compatible with `armv7-sony-vita-newlibeabihf` target.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use p256::pkcs8::{EncodePrivateKey, EncodePublicKey};
use p256::SecretKey;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error as TlsError, SignatureScheme,
    StreamOwned,
};
use sha2::{Digest, Sha256};

use crate::bep::{
    read_bep_message, write_bep_message, BepClose, BepClusterConfig, BepFileInfo, BepFolder,
    BepHello, BepIndex, BepMessageType, BepRequest, BepResponse,
};
use crate::disco::DeviceId;
use crate::storage::FsSyncStorage;

/// Official ALPN required by Syncthing for BEP/1.0 protocol
pub const BEP_ALPN: &[u8] = b"bep/1.0";

/// Cryptographic identity of a Syncthing node (X.509 DER certificate + PKCS#8 DER private key + `DeviceId`).
#[derive(Clone)]
pub struct VitaIdentity {
    pub cert_der: Vec<u8>,
    pub key_pkcs8_der: Vec<u8>,
    pub device_id: DeviceId,
}

impl VitaIdentity {
    pub fn from_seed(seed: &[u8; 32]) -> io::Result<Self> {
        let secret_key = SecretKey::from_bytes(seed.into()).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("Invalid P-256 seed: {e}"))
        })?;
        let signing_key = SigningKey::from(&secret_key);

        let pkcs8_doc = secret_key.to_pkcs8_der().map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("PKCS#8 encoding error: {e}"))
        })?;
        let key_pkcs8_der = pkcs8_doc.as_bytes().to_vec();

        let spki_doc = secret_key.public_key().to_public_key_der().map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("SPKI encoding error: {e}"))
        })?;
        let spki_der = spki_doc.as_bytes();

        let cert_der = build_self_signed_x509_der(&signing_key, spki_der);

        let mut id_bytes = [0u8; 32];
        let digest = Sha256::digest(&cert_der);
        id_bytes.copy_from_slice(&digest);

        Ok(Self {
            cert_der,
            key_pkcs8_der,
            device_id: DeviceId(id_bytes),
        })
    }
}

/// Real-time events emitted during a BEP session (for logger and PS Vita GUI).
#[derive(Debug, Clone)]
pub enum SyncEvent {
    PeerConnected {
        peer_addr: SocketAddr,
        peer_device_id: DeviceId,
        peer_name: String,
        client_version: String,
    },
    ClusterConfigReceived {
        folders: Vec<BepFolder>,
    },
    FileUpToDate {
        folder: String,
        name: String,
        size: u64,
    },
    BlockDownloaded {
        folder: String,
        name: String,
        offset: u64,
        size: usize,
        total_size: u64,
    },
    FileSynced {
        folder: String,
        name: String,
        blocks_downloaded: usize,
        bytes_downloaded: usize,
        total_size: u64,
    },
    LocalIndexPublished {
        folder: String,
        file_count: usize,
    },
    BlockUploaded {
        folder: String,
        name: String,
        offset: u64,
        size: usize,
    },
}

/// Final status of the post-`Hello` handshake session.
#[derive(Debug, Clone)]
pub enum SyncSessionStatus {
    AwaitingPairing,
    ClosedByPeer(String),
    Active {
        shared_folders: Vec<BepFolder>,
        files_up_to_date: usize,
        files_downloaded: usize,
        bytes_downloaded: usize,
        blocks_uploaded: usize,
        bytes_uploaded: usize,
    },
}

/// Complete result of a TLS + BEP/1.0 session with a Syncthing peer.
#[derive(Debug, Clone)]
pub struct PeerHandshakeResult {
    pub peer_addr: SocketAddr,
    pub peer_device_id: DeviceId,
    pub peer_hello: BepHello,
    pub tls_version: String,
    pub alpn: Option<String>,
    pub session_status: SyncSessionStatus,
}

#[derive(Debug)]
struct SyncthingServerVerifier {
    provider: Arc<CryptoProvider>,
    expected_device_id: Option<DeviceId>,
    observed_device_id: Arc<Mutex<Option<DeviceId>>>,
}

impl ServerCertVerifier for SyncthingServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let digest = Sha256::digest(end_entity.as_ref());
        let mut raw_id = [0u8; 32];
        raw_id.copy_from_slice(&digest);
        let actual_id = DeviceId(raw_id);

        if let Some(expected) = self.expected_device_id {
            if actual_id != expected {
                return Err(TlsError::General(format!(
                    "Peer TLS DeviceId ({actual_id}) != expected DeviceId ({expected})"
                )));
            }
        }

        if let Ok(mut guard) = self.observed_device_id.lock() {
            *guard = Some(actual_id);
        }

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Serves an incoming `BepRequest` from local PS Vita storage.
fn handle_incoming_request<W: io::Write>(
    stream: &mut W,
    storage: &FsSyncStorage,
    payload: &[u8],
    blocks_uploaded: &mut usize,
    bytes_uploaded: &mut usize,
    on_event: &mut dyn FnMut(SyncEvent),
) -> io::Result<()> {
    let req = BepRequest::decode(payload)?;
    match storage.read_block_for_peer(
        &req.folder,
        &req.name,
        req.offset.max(0) as u64,
        req.size.max(0) as usize,
    ) {
        Ok(data) => {
            let len = data.len();
            let resp = BepResponse {
                id: req.id,
                data,
                code: 0, // NO_ERROR
            };
            write_bep_message(stream, BepMessageType::Response, &resp.encode())?;
            *blocks_uploaded += 1;
            *bytes_uploaded += len;
            on_event(SyncEvent::BlockUploaded {
                folder: req.folder,
                name: req.name,
                offset: req.offset.max(0) as u64,
                size: len,
            });
        }
        Err(_) => {
            let resp = BepResponse {
                id: req.id,
                data: Vec::new(),
                code: 2, // ERROR_CODE_NO_SUCH_FILE
            };
            write_bep_message(stream, BepMessageType::Response, &resp.encode())?;
        }
    }
    Ok(())
}

/// Probes a TCP endpoint with a TLS 1.3 (`bep/1.0` ALPN) handshake and returns its Syncthing `DeviceId`
/// (SHA-256 of the server's X.509 certificate) if it is a valid Syncthing BEP peer.
pub fn probe_bep_peer_device_id(peer_addr: SocketAddr, timeout: Duration) -> Option<DeviceId> {
    let tcp = TcpStream::connect_timeout(&peer_addr, timeout).ok()?;
    let _ = tcp.set_read_timeout(Some(timeout));
    let _ = tcp.set_write_timeout(Some(timeout));

    let provider = Arc::new(rustls_rustcrypto::provider());
    let observed_device_id = Arc::new(Mutex::new(None));

    let verifier = Arc::new(SyncthingServerVerifier {
        provider: Arc::clone(&provider),
        expected_device_id: None,
        observed_device_id: Arc::clone(&observed_device_id),
    });

    let mut client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    client_config.alpn_protocols = vec![BEP_ALPN.to_vec()];

    let server_name = ServerName::try_from("syncthing").ok()?;
    let mut conn = ClientConnection::new(Arc::new(client_config), server_name).ok()?;
    let mut tcp = tcp;

    while conn.is_handshaking() {
        if conn.complete_io(&mut tcp).is_err() {
            break;
        }
    }

    let id = observed_device_id.lock().ok().and_then(|g| *g);
    id
}

/// Establishes TLS 1.3 + BEP/1.0 session and executes:
/// 1. Delta Sync download with 128 KiB streaming disk writes.
/// 2. Local indexing of target folder and publishing Vita's local `Index`.
/// 3. Block server (`Request` -> `Response`) to upload Vita files to remote peers.
pub fn connect_and_sync<F>(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    our_hello: &BepHello,
    storage: &FsSyncStorage,
    timeout: Duration,
    mut on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    let tcp = TcpStream::connect_timeout(&peer_addr, timeout)?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;

    let provider = Arc::new(rustls_rustcrypto::provider());
    let observed_device_id = Arc::new(Mutex::new(None));

    let verifier = Arc::new(SyncthingServerVerifier {
        provider: Arc::clone(&provider),
        expected_device_id,
        observed_device_id: Arc::clone(&observed_device_id),
    });

    let cert_chain = vec![CertificateDer::from(identity.cert_der.clone())];
    let priv_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone()));

    let mut client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("TLS version error: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(cert_chain, priv_key)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("Client cert error: {e}")))?;

    client_config.alpn_protocols = vec![BEP_ALPN.to_vec()];

    let server_name = ServerName::try_from("syncthing")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("ServerName: {e}")))?;

    let conn = ClientConnection::new(Arc::new(client_config), server_name)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("TLS init failed: {e}")))?;

    let mut tls_stream = StreamOwned::new(conn, tcp);

    // 1. Exchange BEP/1.0 Hello message
    let peer_hello = match our_hello.exchange(&mut tls_stream) {
        Ok(hello) => hello,
        Err(e) => {
            if let Some(peer_device_id) = observed_device_id.lock().ok().and_then(|g| *g) {
                let fallback_hello = BepHello {
                    device_name: format!("Syncthing ({})", peer_addr.ip()),
                    client_name: "syncthing".to_owned(),
                    client_version: "TLS 1.3".to_owned(),
                };
                on_event(SyncEvent::PeerConnected {
                    peer_addr,
                    peer_device_id,
                    peer_name: fallback_hello.device_name.clone(),
                    client_version: fallback_hello.client_version.clone(),
                });
                return Ok(PeerHandshakeResult {
                    peer_addr,
                    peer_device_id,
                    peer_hello: fallback_hello,
                    tls_version: "TLSv1_3".to_owned(),
                    alpn: Some("bep/1.0".to_owned()),
                    session_status: SyncSessionStatus::AwaitingPairing,
                });
            }
            return Err(e);
        }
    };

    let tls_version = tls_stream
        .conn
        .protocol_version()
        .map(|v| format!("{v:?}"))
        .unwrap_or_else(|| "Unknown".to_owned());

    let alpn = tls_stream
        .conn
        .alpn_protocol()
        .map(|p| String::from_utf8_lossy(p).into_owned());

    let peer_device_id = observed_device_id
        .lock()
        .ok()
        .and_then(|g| *g)
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Peer certificate was not observed"))?;

    on_event(SyncEvent::PeerConnected {
        peer_addr,
        peer_device_id,
        peer_name: peer_hello.device_name.clone(),
        client_version: peer_hello.client_version.clone(),
    });

    // 2. Initial read of ClusterConfig and remote Index messages
    let _ = tls_stream
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(1200)));

    let mut shared_folders: Vec<BepFolder> = Vec::new();
    let mut peer_folder_ids: Vec<String> = Vec::new();
    let mut remote_files_by_folder: HashMap<String, Vec<BepFileInfo>> = HashMap::new();
    let mut got_cluster_config = false;
    let mut blocks_uploaded = 0usize;
    let mut bytes_uploaded = 0usize;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(4) {
        match read_bep_message(&mut tls_stream) {
            Ok((BepMessageType::ClusterConfig, payload)) => {
                got_cluster_config = true;
                let cfg = BepClusterConfig::decode(&payload)?;
                peer_folder_ids = cfg.folders.iter().map(|f| f.id.clone()).collect();
                shared_folders = cfg.folders;

                // Merge locally created shares that are shared with this peer_device_id
                for lf in storage.local_folders() {
                    if (lf.device_ids.is_empty() || lf.device_ids.contains(&peer_device_id))
                        && !shared_folders.iter().any(|f| f.id == lf.id)
                    {
                        shared_folders.push(lf.clone());
                    }
                }

                on_event(SyncEvent::ClusterConfigReceived {
                    folders: shared_folders.clone(),
                });

                // Compute local index states so our ClusterConfig advertises accurate index_id & max_sequence
                let mut initial_states = Vec::new();
                for f in &shared_folders {
                    let state = if storage.is_folder_paused(&f.id) {
                        crate::bep::FolderIndexState {
                            index_id: 0,
                            max_sequence: 0,
                        }
                    } else {
                        storage
                            .build_local_index(&f.id, identity.device_id, &[])
                            .map(|(_, st)| st)
                            .unwrap_or(crate::bep::FolderIndexState {
                                index_id: 0,
                                max_sequence: 0,
                            })
                    };
                    initial_states.push((f.clone(), state));
                }
                let reply = BepClusterConfig::encode_for_peers(
                    &initial_states,
                    identity.device_id,
                    peer_device_id,
                );
                write_bep_message(&mut tls_stream, BepMessageType::ClusterConfig, &reply)?;
            }
            Ok((BepMessageType::Index | BepMessageType::IndexUpdate, payload)) => {
                let idx = BepIndex::decode(&payload)?;
                remote_files_by_folder
                    .entry(idx.folder)
                    .or_default()
                    .extend(idx.files);
            }
            Ok((BepMessageType::Request, payload)) => {
                handle_incoming_request(
                    &mut tls_stream,
                    storage,
                    &payload,
                    &mut blocks_uploaded,
                    &mut bytes_uploaded,
                    &mut on_event,
                )?;
            }
            Ok((BepMessageType::Ping, _)) => {
                write_bep_message(&mut tls_stream, BepMessageType::Ping, &[])?;
            }
            Ok((BepMessageType::Close, payload)) => {
                let close = BepClose::decode(&payload)?;
                return Ok(PeerHandshakeResult {
                    peer_addr,
                    peer_device_id,
                    peer_hello,
                    tls_version,
                    alpn,
                    session_status: SyncSessionStatus::ClosedByPeer(close.reason),
                });
            }
            Ok(_) => {}
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(_) => {
                if !got_cluster_config {
                    return Ok(PeerHandshakeResult {
                        peer_addr,
                        peer_device_id,
                        peer_hello,
                        tls_version,
                        alpn,
                        session_status: SyncSessionStatus::AwaitingPairing,
                    });
                }
                break;
            }
        }
    }

    if !got_cluster_config {
        return Ok(PeerHandshakeResult {
            peer_addr,
            peer_device_id,
            peer_hello,
            tls_version,
            alpn,
            session_status: SyncSessionStatus::AwaitingPairing,
        });
    }

    // 3. Step 1: Delta Sync download with 128 KiB streaming disk writes
    let _ = tls_stream
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(4)));

    let mut next_req_id: i32 = 1;
    let mut files_up_to_date = 0usize;
    let mut files_downloaded = 0usize;
    let mut bytes_downloaded = 0usize;

    for (folder_id, files) in &remote_files_by_folder {
        if storage.is_folder_paused(folder_id) {
            continue;
        }
        for file in files {
            if file.is_dir || file.deleted || file.invalid {
                continue;
            }

            let outdated_blocks = storage.find_outdated_blocks(folder_id, file)?;
            if outdated_blocks.is_empty() {
                files_up_to_date += 1;
                on_event(SyncEvent::FileUpToDate {
                    folder: folder_id.clone(),
                    name: file.name.clone(),
                    size: file.size.max(0) as u64,
                });
                continue;
            }

            let mut file_bytes_dl = 0usize;
            let mut file_blocks_dl = 0usize;
            let mut file_ok = true;

            for block in &outdated_blocks {
                let req_id = next_req_id;
                next_req_id += 1;

                let req = BepRequest {
                    id: req_id,
                    folder: folder_id.clone(),
                    name: file.name.clone(),
                    offset: block.offset,
                    size: block.size,
                    hash: block.hash.clone(),
                };
                write_bep_message(&mut tls_stream, BepMessageType::Request, &req.encode())?;

                loop {
                    let (mtype, payload) = read_bep_message(&mut tls_stream)?;
                    match mtype {
                        BepMessageType::Response => {
                            let resp = BepResponse::decode(&payload)?;
                            if resp.id == req_id {
                                if resp.code != 0 {
                                    file_ok = false;
                                } else {
                                    if block.hash.len() == 32 {
                                        let actual_hash = Sha256::digest(&resp.data);
                                        if actual_hash.as_slice() != block.hash.as_slice() {
                                            return Err(io::Error::new(
                                                io::ErrorKind::InvalidData,
                                                format!("Invalid SHA-256 block hash for {}", file.name),
                                            ));
                                        }
                                    }
                                    // Direct streaming write to disk at offset (RAM footprint = 128 KiB)
                                    storage.write_block_streaming(
                                        folder_id,
                                        &file.name,
                                        block.offset.max(0) as u64,
                                        &resp.data,
                                        file.size.max(0) as u64,
                                    )?;
                                    file_blocks_dl += 1;
                                    file_bytes_dl += resp.data.len();
                                    bytes_downloaded += resp.data.len();
                                    on_event(SyncEvent::BlockDownloaded {
                                        folder: folder_id.clone(),
                                        name: file.name.clone(),
                                        offset: block.offset.max(0) as u64,
                                        size: resp.data.len(),
                                        total_size: file.size.max(0) as u64,
                                    });
                                }
                                break;
                            }
                        }
                        BepMessageType::Request => {
                            handle_incoming_request(
                                &mut tls_stream,
                                storage,
                                &payload,
                                &mut blocks_uploaded,
                                &mut bytes_uploaded,
                                &mut on_event,
                            )?;
                        }
                        BepMessageType::Ping => {
                            write_bep_message(&mut tls_stream, BepMessageType::Ping, &[])?;
                        }
                        _ => {}
                    }
                }

                if !file_ok {
                    break;
                }
            }

            if file_ok {
                files_downloaded += 1;
                on_event(SyncEvent::FileSynced {
                    folder: folder_id.clone(),
                    name: file.name.clone(),
                    blocks_downloaded: file_blocks_dl,
                    bytes_downloaded: file_bytes_dl,
                    total_size: file.size.max(0) as u64,
                });
            }
        }
    }

    // 4. Step 2: Bidirectional local indexing & serving Upload requests (Vita -> Mac/PC)
    let mut folder_states = Vec::new();
    let mut local_indexes = Vec::new();

    for folder in &shared_folders {
        if storage.is_folder_paused(&folder.id) {
            continue;
        }
        let remote_files = remote_files_by_folder
            .get(&folder.id)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let (local_idx, state) =
            storage.build_local_index(&folder.id, identity.device_id, remote_files)?;
        folder_states.push((folder.clone(), state));
        local_indexes.push(local_idx);
    }

    if !remote_files_by_folder.is_empty() {
        let updated_cc =
            BepClusterConfig::encode_for_peers(&folder_states, identity.device_id, peer_device_id);
        write_bep_message(&mut tls_stream, BepMessageType::ClusterConfig, &updated_cc)?;
    }

    // Only publish Index for folders that the remote peer also included in its ClusterConfig.
    // Sending Index for a folder not yet accepted by the peer causes Syncthing v2.x to close
    // the connection with `handling index for <folder>: no such folder`.
    for local_idx in &local_indexes {
        if peer_folder_ids.contains(&local_idx.folder) {
            write_bep_message(
                &mut tls_stream,
                BepMessageType::Index,
                &local_idx.encode(),
            )?;
            let file_count = local_idx.files.iter().filter(|f| !f.is_dir).count();
            on_event(SyncEvent::LocalIndexPublished {
                folder: local_idx.folder.clone(),
                file_count,
            });
        }
    }

    // Active BEP/1.0 session window: keep connection alive with Ping, serve block uploads,
    // and immediately publish our Index if the user accepts a newly offered folder in Syncthing Web UI.
    let _ = tls_stream
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(2000)));
    let upload_window = Instant::now();
    while upload_window.elapsed() < Duration::from_secs(45) {
        match read_bep_message(&mut tls_stream) {
            Ok((BepMessageType::ClusterConfig, payload)) => {
                if let Ok(cfg) = BepClusterConfig::decode(&payload) {
                    for f in cfg.folders {
                        if !peer_folder_ids.contains(&f.id) {
                            peer_folder_ids.push(f.id.clone());
                            if let Some(local_idx) =
                                local_indexes.iter().find(|idx| idx.folder == f.id)
                            {
                                let _ = write_bep_message(
                                    &mut tls_stream,
                                    BepMessageType::Index,
                                    &local_idx.encode(),
                                );
                                let file_count =
                                    local_idx.files.iter().filter(|fi| !fi.is_dir).count();
                                on_event(SyncEvent::LocalIndexPublished {
                                    folder: local_idx.folder.clone(),
                                    file_count,
                                });
                            }
                        }
                    }
                }
            }
            Ok((BepMessageType::Request, payload)) => {
                handle_incoming_request(
                    &mut tls_stream,
                    storage,
                    &payload,
                    &mut blocks_uploaded,
                    &mut bytes_uploaded,
                    &mut on_event,
                )?;
            }
            Ok((BepMessageType::Ping, _)) => {
                write_bep_message(&mut tls_stream, BepMessageType::Ping, &[])?;
            }
            Ok((BepMessageType::Close, _)) => break,
            Ok(_) => {}
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                if write_bep_message(&mut tls_stream, BepMessageType::Ping, &[]).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    Ok(PeerHandshakeResult {
        peer_addr,
        peer_device_id,
        peer_hello,
        tls_version,
        alpn,
        session_status: SyncSessionStatus::Active {
            shared_folders,
            files_up_to_date,
            files_downloaded,
            bytes_downloaded,
            blocks_uploaded,
            bytes_uploaded,
        },
    })
}

fn build_self_signed_x509_der(signing_key: &SigningKey, spki_der: &[u8]) -> Vec<u8> {
    let alg_id = der_seq(&[&[0x06, 0x08, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02]]);
    let version_v3 = &[0xA0, 0x03, 0x02, 0x01, 0x02];
    let serial = &[0x02, 0x01, 0x01];

    let cn_attr = der_seq(&[
        &[0x06, 0x03, 0x55, 0x04, 0x03],
        &der_tlv(0x0C, b"syncthing"),
    ]);
    let rdn_set = der_tlv(0x31, &cn_attr);
    let name = der_seq(&[&rdn_set]);

    let validity = der_seq(&[
        &der_tlv(0x18, b"20240101000000Z"),
        &der_tlv(0x18, b"20491231235959Z"),
    ]);

    let tbs_certificate = der_seq(&[
        version_v3,
        serial,
        &alg_id,
        &name,
        &validity,
        &name,
        spki_der,
    ]);

    let sig: Signature = signing_key.sign(&tbs_certificate);
    let sig_der = sig.to_der();
    let mut bit_string_payload = Vec::with_capacity(1 + sig_der.as_bytes().len());
    bit_string_payload.push(0x00);
    bit_string_payload.extend_from_slice(sig_der.as_bytes());
    let signature_bit_string = der_tlv(0x03, &bit_string_payload);

    der_seq(&[&tbs_certificate, &alg_id, &signature_bit_string])
}

fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let len = content.len();
    let mut out = Vec::with_capacity(4 + len);
    out.push(tag);
    if len < 0x80 {
        out.push(len as u8);
    } else if len <= 0xFF {
        out.push(0x81);
        out.push(len as u8);
    } else {
        out.push(0x82);
        out.push(((len >> 8) & 0xFF) as u8);
        out.push((len & 0xFF) as u8);
    }
    out.extend_from_slice(content);
    out
}

fn der_seq(parts: &[&[u8]]) -> Vec<u8> {
    let total_len: usize = parts.iter().map(|p| p.len()).sum();
    let mut content = Vec::with_capacity(total_len);
    for p in parts {
        content.extend_from_slice(p);
    }
    der_tlv(0x30, &content)
}
