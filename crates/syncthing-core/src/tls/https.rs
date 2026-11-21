//! HTTPS connections to Syncthing infrastructure servers (Global Discovery, relay pool).
//!
//! As in Syncthing, a server URL may pin the server certificate with `?id=<device ID>`
//! (the official discovery servers use a self-signed certificate): a pinned certificate is
//! accepted when its SHA-256 matches; any other one must chain to a public root
//! (`webpki-roots`) and match the host name. Global Discovery announcements also present our
//! own certificate: the server learns our `DeviceId` from it.

use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::WebPkiServerVerifier;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use rustls::pki_types::pem::PemObject;

use super::proxy::HttpProxy;
use super::verifier::PinnedOrWebPkiVerifier;
use super::BepTlsStream;
use crate::cert::DeviceIdentity;
use crate::device_id::DeviceId;

/// How HTTPS requests reach the infrastructure servers. The default connects directly and
/// trusts the public roots (`webpki-roots`) only.
#[derive(Debug, Clone, Default)]
pub struct HttpsClient {
    /// HTTP proxy every request is tunnelled through (`CONNECT`).
    pub proxy: Option<HttpProxy>,
    /// Roots trusted in addition to the public ones (e.g. the CA of a TLS-inspecting proxy).
    pub extra_roots: Vec<CertificateDer<'static>>,
}

impl HttpsClient {
    /// Adds the certificates of a PEM bundle to the trusted roots; returns how many.
    pub fn add_pem_roots(&mut self, pem: &[u8]) -> usize {
        let before = self.extra_roots.len();
        self.extra_roots
            .extend(CertificateDer::pem_slice_iter(pem).filter_map(Result::ok));
        self.extra_roots.len() - before
    }
}

/// Query flags of Syncthing discovery URLs that configure the client (`?nolookup`, ...).
const CLIENT_ONLY_FLAGS: [&str; 3] = ["nolookup", "noannounce", "insecure"];

/// A Syncthing infrastructure server: `https://host[:port]/path[?query]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpsServer {
    pub host: String,
    pub port: u16,
    /// Path and query sent in the request line (e.g. `/v2/`); `id=` and the client-only flags
    /// (`nolookup`, `noannounce`, `insecure`) are kept out.
    pub path: String,
    /// `?id=` pin of the server certificate.
    pub pinned_id: Option<DeviceId>,
}

impl HttpsServer {
    /// Parses `https://host[:port][/path][?query]`; an `id=<device ID>` query parameter pins
    /// the server certificate and is removed from [`Self::path`]. `None` if invalid.
    pub fn parse(url: &str) -> Option<Self> {
        let rest = url.trim().strip_prefix("https://")?;
        let (authority, path_query) = match rest.find(['/', '?']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = split_host_port(authority)?;
        let (path, query) = path_query.split_once('?').unwrap_or((path_query, ""));
        let path = if path.is_empty() { "/" } else { path };
        let mut pinned_id = None;
        let mut kept = Vec::new();
        for param in query.split('&').filter(|p| !p.is_empty()) {
            match param.strip_prefix("id=") {
                // The pin is compared with the full certificate hash: a short ID is invalid.
                Some(id) => {
                    let id = DeviceId::parse_user_id(id).ok()?;
                    if id.is_short_placeholder() {
                        return None;
                    }
                    pinned_id = Some(id);
                }
                // Syncthing's client-side flags, not meant for the server.
                None if CLIENT_ONLY_FLAGS.contains(&param) => {}
                None => kept.push(param),
            }
        }
        let path = if kept.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{}", kept.join("&"))
        };
        Some(Self {
            host,
            port,
            path,
            pinned_id,
        })
    }

    /// `host` or `host:port` for the HTTP `Host` header.
    pub fn host_header(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == 443 {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

/// Splits `host`, `host:port` or `[v6]:port` (default port 443).
fn split_host_port(authority: &str) -> Option<(String, u16)> {
    if authority.is_empty() {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None if after.is_empty() => 443,
            None => return None,
        };
        return Some((host.to_owned(), port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, port.parse().ok()?),
        _ => (authority, 443),
    };
    (!host.is_empty()).then(|| (host.to_owned(), port))
}

/// Filters resolved socket addresses for TCP connection attempts (`SceNet` on PS Vita is IPv4-only).
pub fn select_connect_addrs(
    addrs: impl IntoIterator<Item = SocketAddr>,
    ipv4_only: bool,
) -> Vec<SocketAddr> {
    if ipv4_only {
        addrs.into_iter().filter(SocketAddr::is_ipv4).collect()
    } else {
        addrs.into_iter().collect()
    }
}

fn connect_any(
    server: &HttpsServer,
    client: &HttpsClient,
    timeout: Duration,
) -> io::Result<TcpStream> {
    if let Some(proxy) = &client.proxy {
        return proxy.connect(&server.host, server.port, timeout);
    }
    let addrs = select_connect_addrs(
        (server.host.as_str(), server.port).to_socket_addrs()?,
        cfg!(target_os = "vita"),
    );
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(tcp) => return Ok(tcp),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No address resolved")))
}

/// Opens an HTTPS (`http/1.1`) stream to `server` through `client`, presenting
/// `client_identity`'s certificate when given.
pub fn open_https_stream(
    server: &HttpsServer,
    client: &HttpsClient,
    client_identity: Option<&DeviceIdentity>,
    timeout: Duration,
) -> io::Result<BepTlsStream> {
    let tcp = connect_any(server, client, timeout)?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;

    let provider: Arc<CryptoProvider> = Arc::new(super::crypto_provider());
    let mut roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    roots.add_parsable_certificates(client.extra_roots.iter().cloned());
    let roots = Arc::new(roots);
    let webpki = WebPkiServerVerifier::builder_with_provider(roots, Arc::clone(&provider))
        .build()
        .map_err(|e| io::Error::other(format!("TLS verifier error: {e}")))?;
    let verifier = Arc::new(PinnedOrWebPkiVerifier::new(server.pinned_id, webpki));
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| io::Error::other(format!("TLS version error: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(verifier);
    let mut config = match client_identity {
        Some(identity) => builder
            .with_client_auth_cert(
                vec![CertificateDer::from(identity.cert_der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_pkcs8_der.clone())),
            )
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?,
        None => builder.with_no_client_auth(),
    };
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let name = ServerName::try_from(server.host.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("ServerName: {e}")))?;
    let conn = ClientConnection::new(Arc::new(config), name)
        .map_err(|e| io::Error::other(format!("TLS init failed: {e}")))?;
    Ok(BepTlsStream::Client(StreamOwned::new(conn, tcp)))
}
