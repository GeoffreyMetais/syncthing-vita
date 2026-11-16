//! Local TLS servers standing in for Syncthing infrastructure (HTTPS discovery servers and
//! relays): one connection, one scripted exchange, Syncthing certificates.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use syncthing_core::tls::verifier::{ObservedDeviceId, SyncthingClientVerifier};
use syncthing_core::{DeviceId, DeviceIdentity};

/// Server side of an accepted TLS connection.
pub type ServerStream = StreamOwned<ServerConnection, TcpStream>;

/// I/O timeout of the server side: a client that stops talking fails the test instead of
/// hanging it.
const SERVER_IO_TIMEOUT: Duration = Duration::from_secs(10);

/// A running one-connection TLS server on the loopback interface.
pub struct TlsTestServer<T> {
    pub port: u16,
    pub device_id: DeviceId,
    observed: ObservedDeviceId,
    handle: JoinHandle<T>,
}

impl<T> TlsTestServer<T> {
    /// Waits for the exchange to end. Returns its result and the `DeviceId` of the client
    /// certificate, when the server asked for one and the client presented it.
    pub fn join(self) -> (T, Option<DeviceId>) {
        let result = self.handle.join().expect("TLS test server panicked");
        let presented = *self.observed.lock().expect("observed device ID lock");
        (result, presented)
    }
}

/// Builds an identity from a one-byte seed (`[seed; 32]`).
pub fn test_identity(seed: u8) -> DeviceIdentity {
    DeviceIdentity::from_seed(&[seed; 32]).expect("test identity")
}

/// Starts a TLS server presenting `identity` on a free loopback port, negotiating `alpn`.
/// With `require_client_cert`, the client must present a self-signed certificate (any is
/// accepted and its `DeviceId` recorded, as Syncthing peers do). `exchange` runs once on the
/// accepted connection; the connection is closed when it returns.
pub fn spawn_tls_server<T, F>(
    identity: DeviceIdentity,
    alpn: &[u8],
    require_client_cert: bool,
    exchange: F,
) -> TlsTestServer<T>
where
    F: FnOnce(&mut ServerStream) -> T + Send + 'static,
    T: Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    spawn_tls_server_on(listener, 0, identity, alpn, require_client_cert, exchange)
}

/// Like [`spawn_tls_server`], on a listener the caller bound (e.g. the fixed BEP port). The
/// first `dropped` connections are accepted and closed at once, without a TLS exchange: they
/// model the plain TCP "is the port open?" check that some clients make before the TLS probe.
pub fn spawn_tls_server_on<T, F>(
    listener: TcpListener,
    dropped: usize,
    identity: DeviceIdentity,
    alpn: &[u8],
    require_client_cert: bool,
    exchange: F,
) -> TlsTestServer<T>
where
    F: FnOnce(&mut ServerStream) -> T + Send + 'static,
    T: Send + 'static,
{
    let device_id = identity.device_id;
    let port = listener.local_addr().expect("listener address").port();
    let observed: ObservedDeviceId = Arc::new(Mutex::new(None));

    let provider = Arc::new(rustls_rustcrypto::provider());
    let builder = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("TLS protocol versions");
    let builder = if require_client_cert {
        builder.with_client_cert_verifier(Arc::new(SyncthingClientVerifier::new(
            provider,
            Arc::clone(&observed),
        )))
    } else {
        builder.with_no_client_auth()
    };
    let mut config = builder
        .with_single_cert(
            vec![CertificateDer::from(identity.cert_der.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone())),
        )
        .expect("server certificate");
    config.alpn_protocols = vec![alpn.to_vec()];
    let config = Arc::new(config);

    let handle = std::thread::spawn(move || {
        for _ in 0..dropped {
            let _ = listener.accept();
        }
        let (tcp, _) = listener.accept().expect("accept client");
        tcp.set_read_timeout(Some(SERVER_IO_TIMEOUT))
            .expect("read timeout");
        tcp.set_write_timeout(Some(SERVER_IO_TIMEOUT))
            .expect("write timeout");
        let tls = ServerConnection::new(config).expect("server connection");
        let mut stream = StreamOwned::new(tls, tcp);
        exchange(&mut stream)
    });
    TlsTestServer {
        port,
        device_id,
        observed,
        handle,
    }
}

/// An HTTP/1.1 request as received by a test server.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// Request line and headers, including the terminating blank line.
    pub head: String,
    /// Body, read as `Content-Length` bytes.
    pub body: String,
}

impl HttpRequest {
    /// The request line, e.g. `GET /v2/?device=... HTTP/1.1`.
    pub fn request_line(&self) -> &str {
        self.head.lines().next().unwrap_or("")
    }
}

/// Reads one HTTP/1.1 request (head, then `Content-Length` bytes of body).
pub fn try_read_http_request(stream: &mut ServerStream) -> io::Result<HttpRequest> {
    let mut raw = Vec::new();
    while !raw.ends_with(b"\r\n\r\n") {
        let mut byte = [0u8; 1];
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "client closed before the end of the request head",
            ));
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
        head,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// Writes a complete HTTP/1.1 response with `Connection: close`, then ends the TLS session.
pub fn write_http_response(
    stream: &mut ServerStream,
    status_line: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> io::Result<()> {
    let mut response = format!("HTTP/1.1 {status_line}\r\n");
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    stream.write_all(response.as_bytes())?;
    stream.conn.send_close_notify();
    stream.flush()
}
