//! Minimal HTTP/1.1 client over an HTTPS stream (`Connection: close`), for the Global
//! Discovery and relay pool servers.

use std::io::{self, Read, Write};
use std::time::Duration;

use crate::cert::DeviceIdentity;
use crate::tls::https::{open_https_stream, HttpsServer};

/// Responses larger than this are truncated (discovery and relay pool answers are small).
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// A decoded HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    /// Header lines (`Name: value`), in order.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpResponse {
    /// Value of the first header named `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Sends `method path_and_query` with an optional JSON `body` to `server` and reads the whole
/// response. `client_identity` presents our certificate (Global Discovery announcements).
pub fn https_request(
    server: &HttpsServer,
    method: &str,
    path_and_query: &str,
    body: Option<&str>,
    client_identity: Option<&DeviceIdentity>,
    timeout: Duration,
) -> io::Result<HttpResponse> {
    let mut stream = open_https_stream(server, client_identity, timeout)?;
    let mut request = format!(
        "{method} {path_and_query} HTTP/1.1\r\nHost: {}\r\nUser-Agent: syncthing-core\r\nAccept: application/json\r\nConnection: close\r\n",
        server.host_header()
    );
    if let Some(body) = body {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut raw = Vec::with_capacity(2048);
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() >= MAX_RESPONSE_BYTES {
                    break;
                }
            }
            // A server may close without `close_notify` after `Connection: close`, and an
            // idle one ends with a timeout: what was received is the response.
            Err(_) if !raw.is_empty() => break,
            Err(e) => return Err(e),
        }
    }
    parse_http_response(&String::from_utf8_lossy(&raw))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid HTTP response"))
}

/// Parses an HTTP/1.1 response, decoding a `Transfer-Encoding: chunked` body.
pub fn parse_http_response(raw: &str) -> Option<HttpResponse> {
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
    let mut lines = head.lines();
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let chunked = headers.iter().any(|(n, v)| {
        n.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
    });
    let body = if chunked {
        decode_chunked(body)
    } else {
        body.to_owned()
    };
    Some(HttpResponse {
        status,
        headers,
        body,
    })
}

fn decode_chunked(body: &str) -> String {
    let mut decoded = String::new();
    let mut rest = body;
    while let Some((size_line, after_size)) = rest.split_once("\r\n") {
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_hex, 16) else {
            break;
        };
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
    decoded
}

/// Extracts the string elements of the JSON array `"key": [...]` (no general JSON parser: the
/// discovery answers are flat lists of URIs).
pub fn json_string_array(json: &str, key: &str) -> Vec<String> {
    let quoted = format!("\"{key}\"");
    let Some(key_pos) = json.find(&quoted) else {
        return Vec::new();
    };
    let after_key = &json[key_pos + quoted.len()..];
    let Some(open_bracket) = after_key.find('[') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut chars = after_key[open_bracket + 1..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            ']' => break,
            '"' => {
                let value = read_json_string(&mut chars);
                if !value.is_empty() {
                    out.push(value);
                }
            }
            _ => {}
        }
    }
    out
}

/// Every string value of the key `key` anywhere in `json` (e.g. each relay's `"url"`).
pub fn json_string_values(json: &str, key: &str) -> Vec<String> {
    let quoted = format!("\"{key}\"");
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(pos) = rest.find(&quoted) {
        rest = &rest[pos + quoted.len()..];
        let trimmed = rest.trim_start();
        let Some(after_colon) = trimmed.strip_prefix(':') else {
            continue;
        };
        let Some(value_start) = after_colon.trim_start().strip_prefix('"') else {
            continue;
        };
        let mut chars = value_start.chars();
        out.push(read_json_string(&mut chars));
        rest = chars.as_str();
    }
    out
}

/// Reads a JSON string after its opening quote (simple escapes only).
fn read_json_string(chars: &mut std::str::Chars<'_>) -> String {
    let mut value = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(esc) = chars.next() {
                    value.push(esc);
                }
            }
            '"' => break,
            _ => value.push(c),
        }
    }
    value
}
