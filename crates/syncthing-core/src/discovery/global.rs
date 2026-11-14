//! Syncthing Global Discovery v2 lookup (`https://discovery-lookup.syncthing.net/v2/`).
//!
//! Queries a peer's announced `tcp://` and `relay://` endpoints over TLS so devices outside the
//! local LAN can still discover and connect to each other.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

use crate::device_id::DeviceId;
use crate::tls::client::open_https_tls_stream;

/// Public Syncthing Global Discovery v2 lookup hostname.
pub const GLOBAL_DISCOVERY_HOST: &str = "discovery-lookup.syncthing.net";
/// HTTPS port for Global Discovery v2.
pub const GLOBAL_DISCOVERY_PORT: u16 = 443;

/// A WAN endpoint returned by Global Discovery v2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerEndpoint {
    /// Direct TCP endpoint (e.g. `tcp://203.0.113.10:22000`).
    Direct(SocketAddr),
    /// Syncthing Relay v1 endpoint (e.g. `relay://198.51.100.5:22067/?id=...`).
    Relay {
        addr: SocketAddr,
        device_id: DeviceId,
    },
}

/// Resolves `host_port` (either `ip:port` or `hostname:port`) to a `SocketAddr`.
fn resolve_socket_addr(host_port: &str) -> Option<SocketAddr> {
    if let Ok(sock) = host_port.parse::<SocketAddr>() {
        return (!sock.ip().is_unspecified()).then_some(sock);
    }
    host_port
        .to_socket_addrs()
        .ok()?
        .find(|s| !s.ip().is_unspecified())
}

/// Parses a single Syncthing URI (`tcp://...` or `relay://...`) into a [`PeerEndpoint`].
pub fn parse_endpoint_uri(uri: &str) -> Option<PeerEndpoint> {
    if let Some(rest) = uri.strip_prefix("tcp://") {
        let host_port = rest.split(['/', '?']).next()?;
        let addr = resolve_socket_addr(host_port)?;
        return Some(PeerEndpoint::Direct(addr));
    }
    if let Some(rest) = uri.strip_prefix("relay://") {
        let (authority, query) = match rest.split_once('?') {
            Some((left, right)) => (left.trim_end_matches('/'), right),
            None => return None,
        };
        let addr = resolve_socket_addr(authority)?;
        let id_str = query.split('&').find_map(|kv| kv.strip_prefix("id="))?;
        let device_id = DeviceId::parse_user_id(id_str).ok()?;
        return Some(PeerEndpoint::Relay { addr, device_id });
    }
    None
}

/// Extracts the string elements of `"addresses": [...]` from a Global Discovery v2 JSON body.
pub fn parse_global_discovery_json(json: &str) -> Vec<String> {
    let Some(key_pos) = json.find("\"addresses\"") else {
        return Vec::new();
    };
    let after_key = &json[key_pos + "\"addresses\"".len()..];
    let Some(open_bracket) = after_key.find('[') else {
        return Vec::new();
    };
    let array_body = &after_key[open_bracket + 1..];

    let mut out = Vec::new();
    let mut chars = array_body.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        match ch {
            ']' => break,
            '"' => {
                let mut val = String::new();
                while let Some((_, c)) = chars.next() {
                    if c == '\\' {
                        if let Some((_, esc)) = chars.next() {
                            val.push(esc);
                        }
                    } else if c == '"' {
                        break;
                    } else {
                        val.push(c);
                    }
                }
                if !val.is_empty() {
                    out.push(val);
                }
            }
            _ => {}
        }
    }
    out
}

/// Decodes an HTTP/1.1 response body, handling `Transfer-Encoding: chunked` if present.
fn extract_http_body(raw: &str) -> Option<(u16, String)> {
    let (headers, body) = raw.split_once("\r\n\r\n")?;
    let status_line = headers.lines().next()?;
    let status: u16 = status_line.split_whitespace().nth(1)?.parse().ok()?;
    let is_chunked = headers.lines().any(|l| {
        l.to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
    });
    if !is_chunked {
        return Some((status, body.to_owned()));
    }
    let mut decoded = String::new();
    let mut rest = body;
    loop {
        let (size_line, after_size) = rest.split_once("\r\n")?;
        let size_hex = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        if size == 0 {
            break;
        }
        // `size` comes from the server: a truncated chunk, or one that does not end on a
        // character boundary, ends the body instead of panicking.
        let Some(chunk) = after_size.get(..size) else {
            decoded.push_str(after_size);
            break;
        };
        decoded.push_str(chunk);
        rest = after_size[size..].strip_prefix("\r\n").unwrap_or("");
    }
    Some((status, decoded))
}

/// Queries Global Discovery v2 for `device_id` and returns its announced direct & relay endpoints.
pub fn lookup_global_peer(device_id: DeviceId, timeout: Duration) -> io::Result<Vec<PeerEndpoint>> {
    let mut stream = open_https_tls_stream(GLOBAL_DISCOVERY_HOST, GLOBAL_DISCOVERY_PORT, timeout)?;
    let req = format!(
        "GET /v2/?device={} HTTP/1.1\r\nHost: {}\r\nUser-Agent: VitaSyncthing/0.1\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        device_id.to_syncthing_string(),
        GLOBAL_DISCOVERY_HOST
    );
    stream.write_all(req.as_bytes())?;
    stream.flush()?;

    let mut raw = Vec::with_capacity(2048);
    let mut buf = [0u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() >= 64 * 1024 {
                    break;
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                if !raw.is_empty() {
                    break;
                }
                return Err(e);
            }
            Err(e) => {
                // `rustls` may report UnexpectedEof if server closes without close_notify after Connection: close
                if !raw.is_empty() {
                    break;
                }
                return Err(e);
            }
        }
    }

    let text = String::from_utf8_lossy(&raw);
    let (status, body) = extract_http_body(&text).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid HTTP response from Global Discovery",
        )
    })?;
    if status == 404 {
        return Ok(Vec::new());
    }
    if status != 200 {
        return Err(io::Error::other(format!("Global Discovery HTTP {status}")));
    }

    let uris = parse_global_discovery_json(&body);
    let mut endpoints = Vec::new();
    for uri in uris {
        if let Some(ep) = parse_endpoint_uri(&uri) {
            if !endpoints.contains(&ep) {
                endpoints.push(ep);
            }
        }
    }
    Ok(endpoints)
}
