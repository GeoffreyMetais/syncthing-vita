//! Local stand-ins for Syncthing infrastructure, on 127.0.0.1: TLS services (relays, HTTPS
//! discovery servers), a plain TCP session port of a relay, and the environment isolation the
//! daemon needs so that no test ever reaches the Internet.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use syncthing_cli::config::{save_config, Config, DeviceConfig};
use syncthing_core::tls::verifier::SyncthingClientVerifier;
use syncthing_core::DeviceIdentity;

/// Server side of an accepted TLS connection.
pub type ServerStream = StreamOwned<ServerConnection, TcpStream>;

/// I/O timeout of every server-side connection: a client that stops talking ends its handler
/// instead of blocking it forever.
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// Proxy and CA variables read by the daemon's HTTPS client: removed so that a test talks to
/// its 127.0.0.1 servers directly.
const PROXY_VARS: [&str; 5] = [
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "SSL_CERT_FILE",
];

/// Removes the proxy variables from this test process (children inherit them).
pub fn isolate_proxy_env() {
    for var in PROXY_VARS {
        std::env::remove_var(var);
    }
}

/// Starts a TLS server presenting `identity` on a free loopback port, negotiating `alpn` when
/// it is not empty. `handler` runs on its own thread for every connection; the server runs
/// until the test process exits.
pub fn spawn_tls_service<F>(
    identity: &DeviceIdentity,
    alpn: &[u8],
    require_client_cert: bool,
    handler: F,
) -> u16
where
    F: Fn(ServerStream) + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let port = listener.local_addr().expect("listener address").port();

    let provider = Arc::new(syncthing_core::tls::crypto_provider());
    let builder = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("TLS protocol versions");
    let builder = if require_client_cert {
        let observed = Arc::new(Mutex::new(None));
        builder
            .with_client_cert_verifier(Arc::new(SyncthingClientVerifier::new(provider, observed)))
    } else {
        builder.with_no_client_auth()
    };
    let mut config = builder
        .with_single_cert(
            vec![CertificateDer::from(identity.cert_der.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone())),
        )
        .expect("server certificate");
    if !alpn.is_empty() {
        config.alpn_protocols = vec![alpn.to_vec()];
    }
    let config = Arc::new(config);
    let handler = Arc::new(handler);

    thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let config = Arc::clone(&config);
            let handler = Arc::clone(&handler);
            thread::spawn(move || {
                let _ = tcp.set_read_timeout(Some(IO_TIMEOUT));
                let _ = tcp.set_write_timeout(Some(IO_TIMEOUT));
                if let Ok(tls) = ServerConnection::new(config) {
                    handler(StreamOwned::new(tls, tcp));
                }
            });
        }
    });
    port
}

/// A plain TCP port on loopback serving one connection per call of `handler` (a relay's
/// session port, which carries no TLS of its own).
pub fn spawn_tcp_service<F>(handler: F) -> u16
where
    F: Fn(TcpStream) + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let port = listener.local_addr().expect("listener address").port();
    thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let _ = tcp.set_read_timeout(Some(IO_TIMEOUT));
            let _ = tcp.set_write_timeout(Some(IO_TIMEOUT));
            handler(tcp);
        }
    });
    port
}

// ---------------------------------------------------------------------------
// HTTPS (Global Discovery)
// ---------------------------------------------------------------------------

/// An HTTP/1.1 request as received by a test server.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// Request line, e.g. `GET /v2/?device=... HTTP/1.1`.
    pub request_line: String,
    /// Request head, including the request line.
    pub head: String,
    /// Body, read as `Content-Length` bytes.
    pub body: String,
}

/// Reads one HTTP/1.1 request (head, then `Content-Length` bytes of body).
pub fn read_http_request(stream: &mut ServerStream) -> io::Result<HttpRequest> {
    let mut raw = Vec::new();
    while !raw.ends_with(b"\r\n\r\n") {
        let mut byte = [0u8; 1];
        if stream.read(&mut byte)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        raw.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&raw).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    Ok(HttpRequest {
        request_line: head.lines().next().unwrap_or("").to_owned(),
        head,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// A response of an HTTPS test server: status line (`200 OK`), headers, body.
#[derive(Debug, Clone)]
pub struct HttpReply {
    pub status: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpReply {
    pub fn ok(body: &str) -> Self {
        Self {
            status: "200 OK".to_owned(),
            headers: Vec::new(),
            body: body.to_owned(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn status(status: &str) -> Self {
        Self {
            status: status.to_owned(),
            headers: Vec::new(),
            body: String::new(),
        }
    }
}

/// Writes a complete HTTP/1.1 response with `Connection: close`, then ends the TLS session.
fn write_http_reply(stream: &mut ServerStream, reply: &HttpReply) -> io::Result<()> {
    let mut response = format!("HTTP/1.1 {}\r\n", reply.status);
    for (name, value) in &reply.headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.body.len(),
        reply.body
    ));
    stream.write_all(response.as_bytes())?;
    stream.conn.send_close_notify();
    stream.flush()
}

/// Starts a Global Discovery stand-in presenting `identity` (its `?id=` pin is the device ID).
/// `respond` answers each request; the returned log holds every request received, in order.
pub fn spawn_https_service<F>(
    identity: &DeviceIdentity,
    require_client_cert: bool,
    respond: F,
) -> (u16, Arc<Mutex<Vec<HttpRequest>>>)
where
    F: Fn(&HttpRequest) -> HttpReply + Send + Sync + 'static,
{
    let log: Arc<Mutex<Vec<HttpRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&log);
    let port = spawn_tls_service(identity, &[], require_client_cert, move |mut stream| {
        let Ok(request) = read_http_request(&mut stream) else {
            return;
        };
        let reply = respond(&request);
        sink.lock().unwrap_or_else(|p| p.into_inner()).push(request);
        let _ = write_http_reply(&mut stream, &reply);
    });
    (port, log)
}

/// Number of requests recorded in `log`.
pub fn request_count(log: &Arc<Mutex<Vec<HttpRequest>>>) -> usize {
    log.lock().unwrap_or_else(|p| p.into_inner()).len()
}

/// `https://127.0.0.1:<port>/v2/?<query>&id=<pin>`: a server URL with its certificate pinned.
pub fn https_url(port: u16, pin: &DeviceIdentity, query: &str) -> String {
    format!(
        "https://127.0.0.1:{port}/v2/?{query}&id={}",
        pin.device_id.to_syncthing_string()
    )
}

// ---------------------------------------------------------------------------
// Relay Protocol v1 frames
// ---------------------------------------------------------------------------

pub const MSG_PING: i32 = 0;
pub const MSG_PONG: i32 = 1;
pub const MSG_JOIN_RELAY_REQUEST: i32 = 2;
pub const MSG_JOIN_SESSION_REQUEST: i32 = 3;
pub const MSG_RESPONSE: i32 = 4;
pub const MSG_SESSION_INVITATION: i32 = 6;

fn push_xdr_opaque(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(data);
    let pad = (4 - (data.len() % 4)) % 4;
    buf.extend(std::iter::repeat_n(0u8, pad));
}

/// One frame: magic, type, payload length, payload.
pub fn frame(msg_type: i32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(&0x9E79_BC40u32.to_be_bytes());
    out.extend_from_slice(&msg_type.to_be_bytes());
    out.extend_from_slice(&(payload.len() as i32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Reads one frame and returns its type and payload.
pub fn read_frame(stream: &mut impl Read) -> io::Result<(i32, Vec<u8>)> {
    let mut header = [0u8; 12];
    stream.read_exact(&mut header)?;
    let msg_type = i32::from_be_bytes(header[4..8].try_into().expect("4 bytes"));
    let len = i32::from_be_bytes(header[8..12].try_into().expect("4 bytes"));
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    stream.read_exact(&mut payload)?;
    Ok((msg_type, payload))
}

/// A `Response` frame with `code` and `message`.
pub fn response_frame(code: i32, message: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&code.to_be_bytes());
    push_xdr_opaque(&mut payload, message.as_bytes());
    frame(MSG_RESPONSE, &payload)
}

/// A `SessionInvitation` frame from device `from`, to be joined at `port` on the relay's
/// address (an empty `address` means the relay's own).
pub fn invitation_frame(from: &[u8], key: &[u8], port: u16, server_socket: bool) -> Vec<u8> {
    let mut payload = Vec::new();
    push_xdr_opaque(&mut payload, from);
    push_xdr_opaque(&mut payload, key);
    push_xdr_opaque(&mut payload, &[]);
    payload.extend_from_slice(&u32::from(port).to_be_bytes());
    payload.extend_from_slice(&u32::from(server_socket).to_be_bytes());
    frame(MSG_SESSION_INVITATION, &payload)
}

/// ALPN of the Relay Protocol v1.
pub const RELAY_ALPN: &[u8] = b"bep-relay";

/// A relay stand-in: it answers the join request with success, then runs `after_join` on the
/// joined connection (which it ends by returning).
pub fn spawn_relay<F>(identity: &DeviceIdentity, after_join: F) -> u16
where
    F: Fn(&mut ServerStream) + Send + Sync + 'static,
{
    spawn_tls_service(identity, RELAY_ALPN, true, move |mut stream| {
        match read_frame(&mut stream) {
            Ok((MSG_JOIN_RELAY_REQUEST, _)) => {}
            _ => return,
        }
        if stream.write_all(&response_frame(0, "success")).is_err() {
            return;
        }
        after_join(&mut stream);
    })
}

/// Reads frames until the connection ends (the daemon going away, or the read timeout).
pub fn drain_frames(stream: &mut impl Read) {
    while read_frame(stream).is_ok() {}
}

/// Writes a configuration in which `peer_id` is the only device, at `addresses`, announcing to
/// and looking up on exactly `announce` and `lookup` (no default server is contacted).
pub fn save_peer_config(
    home: &Path,
    peer_id: &str,
    addresses: &[&str],
    announce: &[String],
    lookup: &[String],
) {
    let mut config = Config::default();
    config.devices.push(DeviceConfig {
        id: peer_id.to_owned(),
        name: "laptop".to_owned(),
        addresses: addresses.iter().map(|a| (*a).to_owned()).collect(),
    });
    config.global_announce_servers = announce.to_vec();
    config.global_lookup_servers = lookup.to_vec();
    save_config(home, &config).expect("save configuration");
}
