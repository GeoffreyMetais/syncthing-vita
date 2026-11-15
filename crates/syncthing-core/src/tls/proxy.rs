//! HTTP proxy tunnels (`CONNECT host:port`) for the HTTPS requests to infrastructure servers,
//! for networks where only a proxy reaches the Internet (Syncthing honours `https_proxy` too).

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Longest proxy response header accepted.
const MAX_PROXY_RESPONSE: usize = 16 * 1024;

/// An HTTP proxy: `http://[user:password@]host:port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpProxy {
    pub host: String,
    pub port: u16,
    /// `Proxy-Authorization` value (`Basic ...`) when the URL has credentials.
    authorization: Option<String>,
}

impl HttpProxy {
    /// Parses `http://[user:password@]host[:port]` (default port 80; the scheme may be
    /// omitted). `None` for other schemes (e.g. `socks5://`) and invalid URLs.
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let rest = match url.split_once("://") {
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => rest,
            Some(_) => return None,
            None => url,
        };
        let authority = rest.split('/').next()?;
        let (credentials, host_port) = match authority.rsplit_once('@') {
            Some((c, h)) => (Some(c), h),
            None => (None, authority),
        };
        let (host, port) = match host_port.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') || h.ends_with(']') => (h, p.parse().ok()?),
            _ => (host_port, 80),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return None;
        }
        Some(Self {
            host: host.to_owned(),
            port,
            authorization: credentials.map(|c| format!("Basic {}", base64(c.as_bytes()))),
        })
    }

    /// Opens a tunnel through the proxy to `host:port`.
    pub fn connect(&self, host: &str, port: u16, timeout: Duration) -> io::Result<TcpStream> {
        let addrs: Vec<SocketAddr> = (self.host.as_str(), self.port).to_socket_addrs()?.collect();
        let addr = addrs
            .first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Proxy address not resolved"))?;
        let mut tcp = TcpStream::connect_timeout(addr, timeout)?;
        tcp.set_read_timeout(Some(timeout))?;
        tcp.set_write_timeout(Some(timeout))?;

        let target = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
        if let Some(auth) = &self.authorization {
            request.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
        }
        request.push_str("\r\n");
        tcp.write_all(request.as_bytes())?;
        tcp.flush()?;

        // Read the response header byte by byte: what follows belongs to the tunnel.
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if tcp.read(&mut byte)? == 0 || head.len() >= MAX_PROXY_RESPONSE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid proxy response",
                ));
            }
            head.push(byte[0]);
        }
        let status_line = String::from_utf8_lossy(&head);
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        if !(200..300).contains(&status) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("Proxy refused CONNECT {target} (HTTP {status})"),
            ));
        }
        Ok(tcp)
    }
}

/// Standard Base64 (for `Proxy-Authorization: Basic`).
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
