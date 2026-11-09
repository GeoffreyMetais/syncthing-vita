//! TLS 1.3 layer and bidirectional BEP/1.0 synchronization engine (`1+2` multi-connection & persistent session).
//! Uses pure Rust primitives (`rustls` + `rustls-rustcrypto` + `p256` + `sha2`)
//! compatible with `armv7-sony-vita-newlibeabihf` target.

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error as TlsError, SignatureScheme,
    StreamOwned,
};
use sha2::{Digest, Sha256};

use crate::bep::{
    read_bep_message, read_bep_message_after_first_byte, write_bep_message, BepClose,
    BepClusterConfig, BepFileInfo, BepFolder, BepHello, BepIndex, BepMessageType, BepRequest,
    BepResponse, FolderIndexState,
};
pub use crate::cert::VitaIdentity;
use crate::disco::DeviceId;
use crate::storage::FsSyncStorage;

/// Official ALPN required by Syncthing for BEP/1.0 protocol.
pub const BEP_ALPN: &[u8] = b"bep/1.0";

/// Thread-safe control handle allowing the UI thread to request an immediate local folder rescan
/// or a clean shutdown of a persistent BEP/1.0 session.
#[derive(Debug, Default)]
pub struct SyncControl {
    stop_requested: AtomicBool,
    rescan_requested: AtomicBool,
    rescan_epoch: AtomicU64,
}

impl SyncControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }

    pub fn is_stop_requested(&self) -> bool {
        self.stop_requested.load(Ordering::SeqCst)
    }

    pub fn request_rescan(&self) {
        self.rescan_requested.store(true, Ordering::SeqCst);
        self.rescan_epoch.fetch_add(1, Ordering::SeqCst);
    }

    pub fn take_rescan_request(&self) -> bool {
        self.rescan_requested.swap(false, Ordering::SeqCst)
    }

    pub fn rescan_epoch(&self) -> u64 {
        self.rescan_epoch.load(Ordering::SeqCst)
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
        peer_device_id: DeviceId,
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
    SyncCycleCompleted {
        active_connections: usize,
        files_up_to_date: usize,
        files_downloaded: usize,
        bytes_downloaded: usize,
        blocks_uploaded: usize,
        bytes_uploaded: usize,
    },
    /// A shared folder's local directory cannot be used; it is skipped for this session.
    FolderUnavailable {
        folder: String,
        error: String,
    },
    /// A remote change was applied without transfer (directory created, deletion, conflict copy).
    LocalChangeApplied {
        folder: String,
        description: String,
    },
    /// A single file could not be synchronized (e.g. local filesystem error); the session continues.
    FileError {
        folder: String,
        name: String,
        error: String,
    },
    /// The persistent session with a peer ended (with the reason, for diagnostics).
    SessionEnded {
        peer_device_id: DeviceId,
        reason: String,
    },
}

/// Prefixes an I/O error with a human-readable stage/context while preserving its kind.
fn with_context(stage: &str, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{stage}: {e}"))
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
            if !expected.matches_peer(&actual_id) {
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

struct SecondaryBepConn {
    stream: StreamOwned<ClientConnection, TcpStream>,
    promoted: bool,
}

/// Opens a client-authenticated TLS 1.3 stream (`ALPN = "bep/1.0"`) to `peer_addr`.
fn open_bep_tls_stream(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    timeout: Duration,
) -> io::Result<(
    StreamOwned<ClientConnection, TcpStream>,
    Arc<Mutex<Option<DeviceId>>>,
)> {
    let tcp = TcpStream::connect_timeout(&peer_addr, timeout)?;
    let _ = tcp.set_nodelay(true);
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
        .map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("Client cert error: {e}"))
        })?;

    client_config.alpn_protocols = vec![BEP_ALPN.to_vec()];

    let server_name = ServerName::try_from("syncthing")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("ServerName: {e}")))?;

    let conn = ClientConnection::new(Arc::new(client_config), server_name)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("TLS init failed: {e}")))?;

    Ok((StreamOwned::new(conn, tcp), observed_device_id))
}

/// Opens a secondary BEP/1.0 data connection (`ClusterConfig { secondary: true }`) for `1+2` mode.
fn open_secondary_bep_stream(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    peer_device_id: DeviceId,
    our_hello: &BepHello,
    timeout: Duration,
) -> io::Result<SecondaryBepConn> {
    let (mut stream, _) =
        open_bep_tls_stream(identity, peer_addr, Some(peer_device_id), timeout)?;
    let _ = our_hello.exchange(&mut stream)?;
    write_bep_message(
        &mut stream,
        BepMessageType::ClusterConfig,
        &BepClusterConfig::encode_secondary(),
    )?;
    Ok(SecondaryBepConn {
        stream,
        promoted: false,
    })
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

/// Returns `true` if `e` represents a non-fatal socket read timeout or would-block condition
/// on standard Unix or PlayStation Vita (`SceNet` / Newlib) sockets.
fn is_socket_timeout_or_would_block(e: &io::Error) -> bool {
    if matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    ) {
        return true;
    }
    if let Some(code) = e.raw_os_error() {
        let u = code as u32;
        if matches!(code, 4 | 11 | 35 | 60 | 116)
            || u == 0x8041_0123
            || u == 0x8041_013C
        {
            return true;
        }
    }
    false
}

/// Polls a TLS BEP stream for the start of a frame within `poll_timeout`.
/// - If no byte arrives within `poll_timeout`, returns `Ok(None)` without consuming any bytes.
/// - As soon as the first byte of a BEP frame arrives, raises the read timeout to `10s` to read
///   the complete frame so a multi-packet frame is never truncated mid-read over PS Vita Wi-Fi.
fn try_read_bep_message(
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
    poll_timeout: Duration,
) -> io::Result<Option<(BepMessageType, Vec<u8>)>> {
    let _ = stream.get_mut().set_read_timeout(Some(poll_timeout));
    let mut first = [0u8; 1];
    loop {
        match stream.read(&mut first) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "BEP peer closed stream",
                ));
            }
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if is_socket_timeout_or_would_block(&e) => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    let _ = stream
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(10)));
    let res = read_bep_message_after_first_byte(stream, first[0]);
    let _ = stream.get_mut().set_read_timeout(Some(poll_timeout));
    res.map(Some)
}

/// Drains all buffered messages (`Request`, `Ping`, secondary `ClusterConfig`) on a secondary stream.
fn poll_secondary_stream(
    sec: &mut SecondaryBepConn,
    poll_timeout: Duration,
    stop_when_promoted: bool,
    storage: &FsSyncStorage,
    blocks_uploaded: &mut usize,
    bytes_uploaded: &mut usize,
    on_event: &mut dyn FnMut(SyncEvent),
) -> io::Result<()> {
    loop {
        match try_read_bep_message(&mut sec.stream, poll_timeout)? {
            None => return Ok(()),
            Some((BepMessageType::ClusterConfig, _)) => {
                sec.promoted = true;
                if stop_when_promoted {
                    return Ok(());
                }
            }
            Some((BepMessageType::Request, payload)) => {
                handle_incoming_request(
                    &mut sec.stream,
                    storage,
                    &payload,
                    blocks_uploaded,
                    bytes_uploaded,
                    on_event,
                )?;
            }
            Some((BepMessageType::Ping, _)) => {
                write_bep_message(&mut sec.stream, BepMessageType::Ping, &[])?;
            }
            Some((BepMessageType::Close, _)) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Secondary connection closed by peer",
                ));
            }
            Some(_) => {}
        }
    }
}

fn merge_remote_index_files(existing: &mut Vec<BepFileInfo>, incoming: &[BepFileInfo]) {
    for file in incoming {
        if let Some(slot) = existing.iter_mut().find(|f| f.name == file.name) {
            *slot = file.clone();
        } else {
            existing.push(file.clone());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_runtime_cluster_config(
    tls_stream: &mut StreamOwned<ClientConnection, TcpStream>,
    storage: &FsSyncStorage,
    own_id: DeviceId,
    peer_device_id: DeviceId,
    payload: &[u8],
    shared_folders: &mut Vec<BepFolder>,
    peer_folder_ids: &mut Vec<String>,
    local_indexes: &mut Vec<BepIndex>,
    last_published_seq: &mut HashMap<String, i64>,
    index_sent_folders: &mut Vec<String>,
    on_event: &mut dyn FnMut(SyncEvent),
) {
    if let Ok(cfg) = BepClusterConfig::decode(payload) {
        if !cfg.secondary {
            let peer_folders = cfg.folders.clone();
            for f in &cfg.folders {
                if !shared_folders.iter().any(|sf| sf.id == f.id) {
                    shared_folders.push(f.clone());
                }
                if !peer_folder_ids.contains(&f.id) {
                    peer_folder_ids.push(f.id.clone());
                    if !storage.is_folder_paused(&f.id)
                        && !local_indexes.iter().any(|idx| idx.folder == f.id)
                    {
                        if let Ok((new_idx, st)) = storage.build_local_index(&f.id, own_id, &[]) {
                            last_published_seq.insert(f.id.clone(), st.max_sequence);
                            local_indexes.push(new_idx);
                        }
                    }
                    if let Some(local_idx) = local_indexes.iter().find(|idx| idx.folder == f.id) {
                        let _ = write_bep_message(
                            tls_stream,
                            BepMessageType::Index,
                            &local_idx.encode(),
                        );
                        if !index_sent_folders.contains(&local_idx.folder) {
                            index_sent_folders.push(local_idx.folder.clone());
                        }
                        let file_count = local_idx.files.iter().filter(|fi| !fi.is_dir && !fi.deleted).count();
                        on_event(SyncEvent::LocalIndexPublished {
                            folder: local_idx.folder.clone(),
                            file_count,
                        });
                    }
                }
            }
            on_event(SyncEvent::ClusterConfigReceived {
                peer_device_id,
                folders: peer_folders,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_runtime_index(
    tls_stream: &mut StreamOwned<ClientConnection, TcpStream>,
    storage: &FsSyncStorage,
    own_id: DeviceId,
    payload: &[u8],
    remote_files_by_folder: &mut HashMap<String, Vec<BepFileInfo>>,
    local_indexes: &mut Vec<BepIndex>,
    last_published_seq: &mut HashMap<String, i64>,
    index_sent_folders: &mut Vec<String>,
    next_req_id: &mut i32,
    files_up_to_date: &mut usize,
    files_downloaded: &mut usize,
    bytes_downloaded: &mut usize,
    blocks_uploaded: &mut usize,
    bytes_uploaded: &mut usize,
    count_up_to_date: bool,
    on_event: &mut dyn FnMut(SyncEvent),
) -> io::Result<bool> {
    if let Ok(idx) = BepIndex::decode(payload) {
        merge_remote_index_files(
            remote_files_by_folder.entry(idx.folder.clone()).or_default(),
            &idx.files,
        );
        if !storage.is_folder_paused(&idx.folder) {
            download_folder_outdated_blocks(
                tls_stream,
                storage,
                own_id,
                &idx.folder,
                &idx.files,
                next_req_id,
                files_up_to_date,
                files_downloaded,
                bytes_downloaded,
                blocks_uploaded,
                bytes_uploaded,
                count_up_to_date,
                on_event,
            )?;
            if let Some(remote_all) = remote_files_by_folder.get(&idx.folder) {
                if let Ok((new_idx, state)) =
                    storage.build_local_index(&idx.folder, own_id, remote_all)
                {
                    last_published_seq.insert(idx.folder.clone(), state.max_sequence);
                    let msg_type = if index_sent_folders.contains(&idx.folder) {
                        BepMessageType::IndexUpdate
                    } else {
                        index_sent_folders.push(idx.folder.clone());
                        BepMessageType::Index
                    };
                    write_bep_message(tls_stream, msg_type, &new_idx.encode())?;
                    let file_count = new_idx.files.iter().filter(|fi| !fi.is_dir && !fi.deleted).count();
                    on_event(SyncEvent::LocalIndexPublished {
                        folder: new_idx.folder.clone(),
                        file_count,
                    });
                    if let Some(slot) = local_indexes.iter_mut().find(|li| li.folder == idx.folder)
                    {
                        *slot = new_idx;
                    } else {
                        local_indexes.push(new_idx);
                    }
                }
            }
            return Ok(true);
        }
    }
    Ok(false)
}

/// Downloads missing or modified 128 KiB blocks for `files` in `folder_id`.
#[allow(clippy::too_many_arguments)]
fn download_folder_outdated_blocks(
    tls_stream: &mut StreamOwned<ClientConnection, TcpStream>,
    storage: &FsSyncStorage,
    own_id: DeviceId,
    folder_id: &str,
    files: &[BepFileInfo],
    next_req_id: &mut i32,
    files_up_to_date: &mut usize,
    files_downloaded: &mut usize,
    bytes_downloaded: &mut usize,
    blocks_uploaded: &mut usize,
    bytes_uploaded: &mut usize,
    count_up_to_date: bool,
    on_event: &mut dyn FnMut(SyncEvent),
) -> io::Result<bool> {
    if storage.is_folder_paused(folder_id) {
        return Ok(false);
    }

    let prev_timeout = tls_stream.get_ref().read_timeout().ok().flatten();
    // Generous per-response timeout: servers on NAS / Docker hosts may take several seconds
    // to read a block from spinning disks before answering.
    let _ = tls_stream
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(20)));

    let mut any_downloaded = false;

    // Version-vector based plan: directories, deletions and identical content are handled
    // locally; only files with a newer remote version are downloaded.
    let plan = match storage.plan_pull(folder_id, own_id, files) {
        Ok(p) => p,
        Err(e) => {
            // Local folder problem (missing / unreadable root): skip this folder only, the
            // session and the other folders keep running.
            let _ = tls_stream.get_mut().set_read_timeout(prev_timeout);
            on_event(SyncEvent::FolderUnavailable {
                folder: folder_id.to_owned(),
                error: e.to_string(),
            });
            return Ok(false);
        }
    };
    for line in &plan.applied {
        on_event(SyncEvent::LocalChangeApplied {
            folder: folder_id.to_owned(),
            description: line.clone(),
        });
        any_downloaded = true;
    }
    if count_up_to_date {
        for file in &plan.up_to_date {
            *files_up_to_date += 1;
            on_event(SyncEvent::FileUpToDate {
                folder: folder_id.to_owned(),
                name: file.name.clone(),
                size: file.size.max(0) as u64,
            });
        }
    }

    for file in &plan.downloads {
        let outdated_blocks = match storage
            .find_outdated_blocks(folder_id, file)
            .and_then(|b| storage.prepare_download(folder_id, &file.name).map(|_| b))
        {
            Ok(b) => b,
            Err(e) => {
                on_event(SyncEvent::FileError {
                    folder: folder_id.to_owned(),
                    name: file.name.clone(),
                    error: e.to_string(),
                });
                continue;
            }
        };

        let mut file_bytes_dl = 0usize;
        let mut file_blocks_dl = 0usize;
        let mut file_ok = true;

        for block in &outdated_blocks {
            let req_id = *next_req_id;
            *next_req_id = next_req_id.wrapping_add(1).max(1);

            let req = BepRequest {
                id: req_id,
                folder: folder_id.to_owned(),
                name: file.name.clone(),
                offset: block.offset,
                size: block.size,
                hash: block.hash.clone(),
            };
            write_bep_message(tls_stream, BepMessageType::Request, &req.encode())?;

            loop {
                let (mtype, payload) = read_bep_message(tls_stream)?;
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
                                        let _ = tls_stream.get_mut().set_read_timeout(prev_timeout);
                                        storage.abort_download(folder_id, &file.name);
                                        return Err(io::Error::new(
                                            io::ErrorKind::InvalidData,
                                            format!("Invalid SHA-256 block hash for {}", file.name),
                                        ));
                                    }
                                }
                                if let Err(e) = storage.write_block_streaming(
                                    folder_id,
                                    &file.name,
                                    block.offset.max(0) as u64,
                                    &resp.data,
                                    file.size.max(0) as u64,
                                ) {
                                    // Local filesystem error (invalid name on exFAT, path clash, ...):
                                    // skip this file but keep the BEP session alive.
                                    on_event(SyncEvent::FileError {
                                        folder: folder_id.to_owned(),
                                        name: file.name.clone(),
                                        error: e.to_string(),
                                    });
                                    file_ok = false;
                                    break;
                                }
                                file_blocks_dl += 1;
                                file_bytes_dl += resp.data.len();
                                *bytes_downloaded += resp.data.len();
                                on_event(SyncEvent::BlockDownloaded {
                                    folder: folder_id.to_owned(),
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
                            tls_stream,
                            storage,
                            &payload,
                            blocks_uploaded,
                            bytes_uploaded,
                            on_event,
                        )?;
                    }
                    BepMessageType::Ping => {
                        write_bep_message(tls_stream, BepMessageType::Ping, &[])?;
                    }
                    _ => {}
                }
            }

            if !file_ok {
                break;
            }
        }

        if file_ok {
            if let Err(e) = storage.commit_download(folder_id, &file.name, file.size.max(0) as u64) {
                storage.abort_download(folder_id, &file.name);
                on_event(SyncEvent::FileError {
                    folder: folder_id.to_owned(),
                    name: file.name.clone(),
                    error: e.to_string(),
                });
                continue;
            }
            storage.record_downloaded(folder_id, own_id, file);
            *files_downloaded += 1;
            any_downloaded = true;
            on_event(SyncEvent::FileSynced {
                folder: folder_id.to_owned(),
                name: file.name.clone(),
                blocks_downloaded: file_blocks_dl,
                bytes_downloaded: file_bytes_dl,
                total_size: file.size.max(0) as u64,
            });
        } else {
            storage.abort_download(folder_id, &file.name);
        }
    }

    let _ = tls_stream.get_mut().set_read_timeout(prev_timeout);
    Ok(any_downloaded)
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

/// Establishes a single-cycle TLS 1.3 + BEP/1.0 session without persistent background keepalive.
pub fn connect_and_sync<F>(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    our_hello: &BepHello,
    storage: &FsSyncStorage,
    timeout: Duration,
    on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    connect_and_sync_with_control(
        identity,
        peer_addr,
        expected_device_id,
        our_hello,
        storage,
        timeout,
        None,
        on_event,
    )
}

/// Establishes TLS 1.3 + BEP/1.0 `1+2` multi-connection session and executes:
/// 1. Primary metadata connection + 2 secondary data connections (`1+2 connections` in Syncthing v2.x).
/// 2. Delta Sync download with 128 KiB streaming disk writes.
/// 3. Local indexing of target folder and publishing Vita's local `Index`.
/// 4. Persistent session loop (when `control` is `Some`), serving block requests, answering `Ping`s,
///    applying live remote `IndexUpdate`s, and rescanning local folders without disconnecting.
#[allow(clippy::too_many_arguments)]
pub fn connect_and_sync_with_control<F>(
    identity: &VitaIdentity,
    peer_addr: SocketAddr,
    expected_device_id: Option<DeviceId>,
    our_hello: &BepHello,
    storage: &FsSyncStorage,
    timeout: Duration,
    control: Option<Arc<SyncControl>>,
    mut on_event: F,
) -> io::Result<PeerHandshakeResult>
where
    F: FnMut(SyncEvent),
{
    let (mut tls_stream, observed_device_id) =
        open_bep_tls_stream(identity, peer_addr, expected_device_id, timeout)
            .map_err(|e| with_context("TCP connect", e))?;

    // 1. Exchange BEP/1.0 Hello message on primary connection
    let peer_hello = match our_hello.exchange(&mut tls_stream) {
        Ok(hello) => hello,
        Err(e) => {
            if let Some(peer_device_id) = observed_device_id.lock().ok().and_then(|g| *g) {
                let fallback_hello = BepHello {
                    device_name: format!("Syncthing ({})", peer_addr.ip()),
                    client_name: "syncthing".to_owned(),
                    client_version: "TLS 1.3".to_owned(),
                    num_connections: 0,
                    timestamp: 0,
                };
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
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Peer certificate was not observed")
        })?;

    // 2. Initial read of ClusterConfig and remote Index messages
    let mut shared_folders: Vec<BepFolder> = Vec::new();
    let mut peer_folder_ids: Vec<String> = Vec::new();
    let mut remote_files_by_folder: HashMap<String, Vec<BepFileInfo>> = HashMap::new();
    let mut got_cluster_config = false;
    let mut blocks_uploaded = 0usize;
    let mut bytes_uploaded = 0usize;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(4) {
        let poll_timeout = if got_cluster_config {
            Duration::from_millis(650)
        } else {
            Duration::from_millis(1600)
        };
        match try_read_bep_message(&mut tls_stream, poll_timeout) {
            Ok(None) => break,
            Ok(Some((BepMessageType::ClusterConfig, payload))) => {
                let cfg = BepClusterConfig::decode(&payload)?;
                if cfg.secondary {
                    continue;
                }
                if !got_cluster_config {
                    got_cluster_config = true;
                    on_event(SyncEvent::PeerConnected {
                        peer_addr,
                        peer_device_id,
                        peer_name: peer_hello.device_name.clone(),
                        client_version: peer_hello.client_version.clone(),
                    });
                }
                peer_folder_ids = cfg.folders.iter().map(|f| f.id.clone()).collect();
                let peer_folders = cfg.folders.clone();
                shared_folders = cfg.folders;

                on_event(SyncEvent::ClusterConfigReceived {
                    peer_device_id,
                    folders: peer_folders,
                });

                // Merge locally offered shares that are explicitly shared with this peer_device_id
                for lf in storage.local_folders() {
                    if lf
                        .device_ids
                        .iter()
                        .any(|id| id.matches_peer(&peer_device_id))
                        && !shared_folders.iter().any(|f| f.id == lf.id)
                    {
                        shared_folders.push(lf.clone());
                    }
                }

                // Compute local index states so our single ClusterConfig advertises accurate index_id & max_sequence
                let mut initial_states = Vec::new();
                for f in &shared_folders {
                    let state = if storage.is_folder_paused(&f.id) {
                        FolderIndexState {
                            index_id: 0,
                            max_sequence: 0,
                            paused: true,
                        }
                    } else {
                        storage
                            .build_local_index(&f.id, identity.device_id, &[])
                            .map(|(_, st)| st)
                            .unwrap_or(FolderIndexState {
                                index_id: 0,
                                max_sequence: 0,
                                paused: false,
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
            Ok(Some((BepMessageType::Index | BepMessageType::IndexUpdate, payload))) => {
                let idx = BepIndex::decode(&payload)?;
                merge_remote_index_files(
                    remote_files_by_folder.entry(idx.folder).or_default(),
                    &idx.files,
                );
            }
            Ok(Some((BepMessageType::Request, payload))) => {
                handle_incoming_request(
                    &mut tls_stream,
                    storage,
                    &payload,
                    &mut blocks_uploaded,
                    &mut bytes_uploaded,
                    &mut on_event,
                )?;
            }
            Ok(Some((BepMessageType::Ping, _))) => {
                write_bep_message(&mut tls_stream, BepMessageType::Ping, &[])?;
            }
            Ok(Some((BepMessageType::Close, payload))) => {
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
            Ok(Some(_)) => {}
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

    // 3. Establish +2 secondary BEP data connections (`1+2 connections`) if the peer supports them
    let desired_secondary = if peer_hello.num_connections > 1 {
        (peer_hello.num_connections.max(our_hello.num_connections) - 1).clamp(0, 2) as usize
    } else {
        0
    };
    let mut secondary_streams: Vec<SecondaryBepConn> = Vec::with_capacity(desired_secondary);
    let secondary_opened_at = Instant::now();
    for _ in 0..desired_secondary {
        if let Ok(sec) = open_secondary_bep_stream(
            identity,
            peer_addr,
            peer_device_id,
            our_hello,
            Duration::from_millis(2000),
        ) {
            secondary_streams.push(sec);
        }
    }

    // 4. Step 1: Delta Sync download with 128 KiB streaming disk writes
    let mut next_req_id: i32 = 1;
    let mut files_up_to_date = 0usize;
    let mut files_downloaded = 0usize;
    let mut bytes_downloaded = 0usize;

    let folder_ids: Vec<String> = remote_files_by_folder.keys().cloned().collect();
    for folder_id in &folder_ids {
        if storage.is_folder_paused(folder_id) {
            continue;
        }
        if let Some(files) = remote_files_by_folder.get(folder_id).cloned() {
            download_folder_outdated_blocks(
                &mut tls_stream,
                storage,
                identity.device_id,
                folder_id,
                &files,
                &mut next_req_id,
                &mut files_up_to_date,
                &mut files_downloaded,
                &mut bytes_downloaded,
                &mut blocks_uploaded,
                &mut bytes_uploaded,
                true,
                &mut on_event,
            )
            .map_err(|e| with_context(&format!("Initial download ({folder_id})"), e))?;
        }
    }

    // 5. Step 2: Bidirectional local indexing & serving Upload requests (Vita -> Mac/PC)
    let mut local_indexes = Vec::new();
    let mut last_published_seq: HashMap<String, i64> = HashMap::new();
    let mut index_sent_folders: Vec<String> = Vec::new();

    for folder in &shared_folders {
        if storage.is_folder_paused(&folder.id) {
            continue;
        }
        let remote_files = remote_files_by_folder
            .get(&folder.id)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let (local_idx, state) =
            match storage.build_local_index(&folder.id, identity.device_id, remote_files) {
                Ok(v) => v,
                Err(e) => {
                    // An unreadable local folder must not take the whole peer session down.
                    on_event(SyncEvent::FolderUnavailable {
                        folder: folder.id.clone(),
                        error: e.to_string(),
                    });
                    continue;
                }
            };
        last_published_seq.insert(folder.id.clone(), state.max_sequence);
        local_indexes.push(local_idx);
    }

    // Wait for the server's 1-second connection promotion timer on secondary connections before
    // publishing our Index so the server can immediately dispatch block Requests over `1+2` connections.
    if !secondary_streams.is_empty() {
        let promotion_budget = Duration::from_millis(1400)
            .saturating_sub(secondary_opened_at.elapsed())
            .max(Duration::from_millis(250));
        let promo_deadline = Instant::now() + promotion_budget;
        for sec in &mut secondary_streams {
            while !sec.promoted && Instant::now() < promo_deadline {
                let rem = promo_deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(350));
                if poll_secondary_stream(
                    sec,
                    rem,
                    true,
                    storage,
                    &mut blocks_uploaded,
                    &mut bytes_uploaded,
                    &mut on_event,
                )
                .is_err()
                {
                    break;
                }
            }
        }
    }

    // NOTE: In BEP/1.0 (Syncthing's `lib/protocol/protocol.go`), sending a second `ClusterConfig`
    // after transitioning to `stateReady` is a fatal protocol error (`cluster config message in state 1`)
    // that causes the remote peer to immediately close all connections. We only send `Index` here.
    for local_idx in &local_indexes {
        if peer_folder_ids.contains(&local_idx.folder) {
            write_bep_message(
                &mut tls_stream,
                BepMessageType::Index,
                &local_idx.encode(),
            )?;
            if !index_sent_folders.contains(&local_idx.folder) {
                index_sent_folders.push(local_idx.folder.clone());
            }
            let file_count = local_idx.files.iter().filter(|f| !f.is_dir).count();
            on_event(SyncEvent::LocalIndexPublished {
                folder: local_idx.folder.clone(),
                file_count,
            });
        }
    }

    // Drain initial upload requests and any late-arriving Index messages before marking initial cycle complete
    let initial_upload_drain = Instant::now();
    let initial_drain_duration = if control.is_some() {
        Duration::from_millis(1000)
    } else {
        Duration::from_secs(4)
    };
    while initial_upload_drain.elapsed() < initial_drain_duration {
        match try_read_bep_message(&mut tls_stream, Duration::from_millis(120)) {
            Ok(None) => {}
            Ok(Some((BepMessageType::ClusterConfig, payload))) => {
                handle_runtime_cluster_config(
                    &mut tls_stream,
                    storage,
                    identity.device_id,
                    peer_device_id,
                    &payload,
                    &mut shared_folders,
                    &mut peer_folder_ids,
                    &mut local_indexes,
                    &mut last_published_seq,
                    &mut index_sent_folders,
                    &mut on_event,
                );
            }
            Ok(Some((BepMessageType::Index | BepMessageType::IndexUpdate, payload))) => {
                let _ = handle_runtime_index(
                    &mut tls_stream,
                    storage,
                    identity.device_id,
                    &payload,
                    &mut remote_files_by_folder,
                    &mut local_indexes,
                    &mut last_published_seq,
                    &mut index_sent_folders,
                    &mut next_req_id,
                    &mut files_up_to_date,
                    &mut files_downloaded,
                    &mut bytes_downloaded,
                    &mut blocks_uploaded,
                    &mut bytes_uploaded,
                    true,
                    &mut on_event,
                )?;
            }
            Ok(Some((BepMessageType::Request, payload))) => {
                handle_incoming_request(
                    &mut tls_stream,
                    storage,
                    &payload,
                    &mut blocks_uploaded,
                    &mut bytes_uploaded,
                    &mut on_event,
                )?;
            }
            Ok(Some((BepMessageType::Ping, _))) => {
                write_bep_message(&mut tls_stream, BepMessageType::Ping, &[])?;
            }
            Ok(Some((BepMessageType::Close, payload))) => {
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
            Ok(Some(_)) => {}
            Err(_) => break,
        }

        let mut sec_idx = 0;
        while sec_idx < secondary_streams.len() {
            if poll_secondary_stream(
                &mut secondary_streams[sec_idx],
                Duration::from_millis(35),
                false,
                storage,
                &mut blocks_uploaded,
                &mut bytes_uploaded,
                &mut on_event,
            )
            .is_err()
            {
                secondary_streams.remove(sec_idx);
            } else {
                sec_idx += 1;
            }
        }
    }

    on_event(SyncEvent::SyncCycleCompleted {
        active_connections: 1 + secondary_streams.len(),
        files_up_to_date,
        files_downloaded,
        bytes_downloaded,
        blocks_uploaded,
        bytes_uploaded,
    });

    // 6. Persistent BEP/1.0 Session Loop: keep `1+2` connections open continuously,
    // answer Ping/Request on all streams, apply remote IndexUpdates, and rescan local folders.
    if let Some(ctrl) = control {
        let mut last_ping = Instant::now();
        let mut last_local_scan = Instant::now();
        let mut last_secondary_retry = Instant::now();
        let mut seen_rescan_epoch = ctrl.rescan_epoch();
        let mut end_reason = String::from("stopped locally");

        'persistent: while !ctrl.is_stop_requested() {
            // A. Poll primary connection
            loop {
                if ctrl.is_stop_requested() {
                    break 'persistent;
                }
                match try_read_bep_message(&mut tls_stream, Duration::from_millis(150)) {
                    Ok(None) => break,
                    Ok(Some((BepMessageType::ClusterConfig, payload))) => {
                        handle_runtime_cluster_config(
                            &mut tls_stream,
                            storage,
                            identity.device_id,
                            peer_device_id,
                            &payload,
                            &mut shared_folders,
                            &mut peer_folder_ids,
                            &mut local_indexes,
                            &mut last_published_seq,
                            &mut index_sent_folders,
                            &mut on_event,
                        );
                    }
                    Ok(Some((BepMessageType::Index | BepMessageType::IndexUpdate, payload))) => {
                        match handle_runtime_index(
                            &mut tls_stream,
                            storage,
                            identity.device_id,
                            &payload,
                            &mut remote_files_by_folder,
                            &mut local_indexes,
                            &mut last_published_seq,
                            &mut index_sent_folders,
                            &mut next_req_id,
                            &mut files_up_to_date,
                            &mut files_downloaded,
                            &mut bytes_downloaded,
                            &mut blocks_uploaded,
                            &mut bytes_uploaded,
                            false,
                            &mut on_event,
                        ) {
                            Ok(true) => {
                                on_event(SyncEvent::SyncCycleCompleted {
                                    active_connections: 1 + secondary_streams.len(),
                                    files_up_to_date,
                                    files_downloaded,
                                    bytes_downloaded,
                                    blocks_uploaded,
                                    bytes_uploaded,
                                });
                            }
                            Ok(false) => {}
                            Err(e) => {
                                end_reason = format!("index/download error: {e}");
                                break 'persistent;
                            }
                        }
                    }
                    Ok(Some((BepMessageType::Request, payload))) => {
                        if let Err(e) = handle_incoming_request(
                            &mut tls_stream,
                            storage,
                            &payload,
                            &mut blocks_uploaded,
                            &mut bytes_uploaded,
                            &mut on_event,
                        ) {
                            end_reason = format!("request handling error: {e}");
                            break 'persistent;
                        }
                    }
                    Ok(Some((BepMessageType::Ping, _))) => {
                        if let Err(e) = write_bep_message(&mut tls_stream, BepMessageType::Ping, &[]) {
                            end_reason = format!("ping reply failed: {e}");
                            break 'persistent;
                        }
                    }
                    Ok(Some((BepMessageType::Close, payload))) => {
                        let reason = BepClose::decode(&payload)
                            .map(|c| c.reason)
                            .unwrap_or_default();
                        end_reason = format!("closed by peer: {reason}");
                        break 'persistent;
                    }
                    Ok(Some(_)) => {}
                    Err(e) => {
                        end_reason = format!("primary connection lost: {e}");
                        break 'persistent;
                    }
                }
            }

            // B. Poll secondary data connections (`+2`)
            let mut sec_idx = 0;
            while sec_idx < secondary_streams.len() {
                if poll_secondary_stream(
                    &mut secondary_streams[sec_idx],
                    Duration::from_millis(35),
                    false,
                    storage,
                    &mut blocks_uploaded,
                    &mut bytes_uploaded,
                    &mut on_event,
                )
                .is_err()
                {
                    secondary_streams.remove(sec_idx);
                } else {
                    sec_idx += 1;
                }
            }

            // C. Maintain `1+2` secondary connections if one dropped transiently
            if secondary_streams.len() < desired_secondary
                && last_secondary_retry.elapsed() >= Duration::from_secs(15)
            {
                last_secondary_retry = Instant::now();
                if let Ok(sec) = open_secondary_bep_stream(
                    identity,
                    peer_addr,
                    peer_device_id,
                    our_hello,
                    Duration::from_millis(1500),
                ) {
                    secondary_streams.push(sec);
                }
            }

            // D. Send periodic Ping keepalives across all active connections
            if last_ping.elapsed() >= Duration::from_secs(20) {
                last_ping = Instant::now();
                if let Err(e) = write_bep_message(&mut tls_stream, BepMessageType::Ping, &[]) {
                    end_reason = format!("keepalive ping failed: {e}");
                    break 'persistent;
                }
                secondary_streams.retain_mut(|sec| {
                    write_bep_message(&mut sec.stream, BepMessageType::Ping, &[]).is_ok()
                });
            }

            // E. Periodic (every 12s) or manual on-demand local folder rescan
            let cur_epoch = ctrl.rescan_epoch();
            let manual_rescan = cur_epoch != seen_rescan_epoch || ctrl.take_rescan_request();
            seen_rescan_epoch = cur_epoch;
            if manual_rescan || last_local_scan.elapsed() >= Duration::from_secs(12) {
                last_local_scan = Instant::now();
                let mut any_changed = false;
                let mut changed_indexes = Vec::new();

                for folder in &shared_folders {
                    if storage.is_folder_paused(&folder.id) {
                        continue;
                    }
                    let remote_files = remote_files_by_folder
                        .get(&folder.id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    if let Ok((local_idx, state)) =
                        storage.build_local_index(&folder.id, identity.device_id, remote_files)
                    {
                        let prev_seq = last_published_seq.get(&folder.id).copied().unwrap_or(0);
                        if state.max_sequence != prev_seq || manual_rescan {
                            if state.max_sequence != prev_seq {
                                any_changed = true;
                            }
                            last_published_seq.insert(folder.id.clone(), state.max_sequence);
                            changed_indexes.push(local_idx.clone());
                        }
                        if let Some(slot) =
                            local_indexes.iter_mut().find(|li| li.folder == folder.id)
                        {
                            *slot = local_idx;
                        } else {
                            local_indexes.push(local_idx);
                        }
                    }
                }

                if any_changed || manual_rescan {
                    for local_idx in &changed_indexes {
                        if peer_folder_ids.contains(&local_idx.folder) {
                            let msg_type = if index_sent_folders.contains(&local_idx.folder) {
                                BepMessageType::IndexUpdate
                            } else {
                                index_sent_folders.push(local_idx.folder.clone());
                                BepMessageType::Index
                            };
                            if let Err(e) =
                                write_bep_message(&mut tls_stream, msg_type, &local_idx.encode())
                            {
                                end_reason = format!("index update send failed: {e}");
                                break 'persistent;
                            }
                            let file_count =
                                local_idx.files.iter().filter(|f| !f.is_dir).count();
                            on_event(SyncEvent::LocalIndexPublished {
                                folder: local_idx.folder.clone(),
                                file_count,
                            });
                        }
                    }
                    on_event(SyncEvent::SyncCycleCompleted {
                        active_connections: 1 + secondary_streams.len(),
                        files_up_to_date,
                        files_downloaded,
                        bytes_downloaded,
                        blocks_uploaded,
                        bytes_uploaded,
                    });
                }
            }
        }
        // Cleanly notify the remote peer when closing the session
        let close_payload = BepClose {
            reason: "closing".to_owned(),
        }
        .encode();
        for mut sec in secondary_streams {
            let _ = write_bep_message(&mut sec.stream, BepMessageType::Close, &close_payload);
        }
        let _ = write_bep_message(&mut tls_stream, BepMessageType::Close, &close_payload);
        on_event(SyncEvent::SessionEnded {
            peer_device_id,
            reason: end_reason,
        });
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
