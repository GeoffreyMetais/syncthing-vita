//! Pure-Rust NAT traversal clients: NAT-PMP (RFC 6886) and UPnP IGD (SSDP + SOAP/HTTP).
//!
//! No external networking or XML crates are used: everything is built directly on `std::net`
//! (`UdpSocket`, `TcpStream`) so it compiles identically on host platforms and PS Vita.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

use super::local_ip::detect_local_ipv4;

/// RFC 6886 NAT-PMP UDP port.
pub const NATPMP_PORT: u16 = 5351;
/// SSDP multicast address for UPnP discovery.
pub const SSDP_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
/// SSDP UDP port.
pub const SSDP_PORT: u16 = 1900;

const NATPMP_VERSION: u8 = 0;
const NATPMP_OP_PUBLIC_ADDR: u8 = 0;
const NATPMP_OP_MAP_TCP: u8 = 2;
const NATPMP_OP_RESPONSE_BIT: u8 = 0x80;
const MAX_HTTP_RESPONSE_BYTES: usize = 256 * 1024;

/// Which NAT port-mapping protocol produced a [`NatMapping`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatProtocol {
    NatPmp,
    UpnpIgd,
}

/// Result of a successful external TCP port mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NatMapping {
    pub external_ip: Ipv4Addr,
    pub external_port: u16,
    pub lease_secs: u32,
    pub protocol: NatProtocol,
}

/// Decoded NAT-PMP public address response (RFC 6886 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NatPmpPublicAddrResponse {
    pub result_code: u16,
    pub epoch_secs: u32,
    pub external_ip: Ipv4Addr,
}

/// Decoded NAT-PMP TCP mapping response (RFC 6886 §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NatPmpMapResponse {
    pub result_code: u16,
    pub epoch_secs: u32,
    pub internal_port: u16,
    pub external_port: u16,
    pub lifetime_secs: u32,
}

/// Encodes a 2-byte NAT-PMP external address request (`version=0, opcode=0`).
pub fn encode_natpmp_public_addr_req() -> [u8; 2] {
    [NATPMP_VERSION, NATPMP_OP_PUBLIC_ADDR]
}

/// Decodes a 12-byte NAT-PMP external address response (RFC 6886 §3.2).
pub fn decode_natpmp_public_addr_resp(buf: &[u8]) -> io::Result<NatPmpPublicAddrResponse> {
    if buf.len() < 12 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "NAT-PMP public address response too short ({} bytes)",
                buf.len()
            ),
        ));
    }
    if buf[0] != NATPMP_VERSION || buf[1] != (NATPMP_OP_PUBLIC_ADDR | NATPMP_OP_RESPONSE_BIT) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unexpected NAT-PMP public address header: version={}, opcode={:#x}",
                buf[0], buf[1]
            ),
        ));
    }
    let result_code = u16::from_be_bytes([buf[2], buf[3]]);
    let epoch_secs = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let external_ip = Ipv4Addr::new(buf[8], buf[9], buf[10], buf[11]);
    Ok(NatPmpPublicAddrResponse {
        result_code,
        epoch_secs,
        external_ip,
    })
}

/// Encodes a 12-byte NAT-PMP TCP mapping request (RFC 6886 §3.3, `opcode=2`).
pub fn encode_natpmp_map_req(
    internal_port: u16,
    external_port: u16,
    lifetime_secs: u32,
) -> [u8; 12] {
    let mut pkt = [0u8; 12];
    pkt[0] = NATPMP_VERSION;
    pkt[1] = NATPMP_OP_MAP_TCP;
    // pkt[2..4] = reserved (0)
    pkt[4..6].copy_from_slice(&internal_port.to_be_bytes());
    pkt[6..8].copy_from_slice(&external_port.to_be_bytes());
    pkt[8..12].copy_from_slice(&lifetime_secs.to_be_bytes());
    pkt
}

/// Decodes a 16-byte NAT-PMP TCP mapping response (RFC 6886 §3.3).
pub fn decode_natpmp_map_resp(buf: &[u8]) -> io::Result<NatPmpMapResponse> {
    if buf.len() < 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("NAT-PMP mapping response too short ({} bytes)", buf.len()),
        ));
    }
    if buf[0] != NATPMP_VERSION || buf[1] != (NATPMP_OP_MAP_TCP | NATPMP_OP_RESPONSE_BIT) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unexpected NAT-PMP map header: version={}, opcode={:#x}",
                buf[0], buf[1]
            ),
        ));
    }
    let result_code = u16::from_be_bytes([buf[2], buf[3]]);
    let epoch_secs = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let internal_port = u16::from_be_bytes([buf[8], buf[9]]);
    let external_port = u16::from_be_bytes([buf[10], buf[11]]);
    let lifetime_secs = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
    Ok(NatPmpMapResponse {
        result_code,
        epoch_secs,
        internal_port,
        external_port,
        lifetime_secs,
    })
}

/// Maps `internal_port` -> `external_port` via NAT-PMP against `gateway:5351`.
pub fn natpmp_map_tcp(
    gateway: Ipv4Addr,
    internal_port: u16,
    external_port: u16,
    lifetime_secs: u32,
    timeout: Duration,
) -> io::Result<NatMapping> {
    natpmp_map_tcp_at(
        SocketAddr::V4(SocketAddrV4::new(gateway, NATPMP_PORT)),
        internal_port,
        external_port,
        lifetime_secs,
        timeout,
    )
}

/// Maps `internal_port` -> `external_port` via NAT-PMP against an explicit `SocketAddr`
/// (useful for tests bound on `127.0.0.1:<ephemeral_port>`).
pub fn natpmp_map_tcp_at(
    gateway_addr: SocketAddr,
    internal_port: u16,
    external_port: u16,
    lifetime_secs: u32,
    timeout: Duration,
) -> io::Result<NatMapping> {
    let bind_addr = match gateway_addr {
        SocketAddr::V4(v4) if v4.ip().is_loopback() => "127.0.0.1:0",
        _ => "0.0.0.0:0",
    };
    let sock = UdpSocket::bind(bind_addr)?;
    sock.set_read_timeout(Some(timeout))?;
    sock.set_write_timeout(Some(timeout))?;

    let addr_req = encode_natpmp_public_addr_req();
    sock.send_to(&addr_req, gateway_addr)?;
    let mut buf = [0u8; 64];
    let (n, _) = sock.recv_from(&mut buf)?;
    let addr_resp = decode_natpmp_public_addr_resp(&buf[..n])?;
    if addr_resp.result_code != 0 {
        return Err(io::Error::other(format!(
            "NAT-PMP public address error code {}",
            addr_resp.result_code
        )));
    }

    let map_req = encode_natpmp_map_req(internal_port, external_port, lifetime_secs);
    sock.send_to(&map_req, gateway_addr)?;
    let (n, _) = sock.recv_from(&mut buf)?;
    let map_resp = decode_natpmp_map_resp(&buf[..n])?;
    if map_resp.result_code != 0 {
        return Err(io::Error::other(format!(
            "NAT-PMP map error code {}",
            map_resp.result_code
        )));
    }

    Ok(NatMapping {
        external_ip: addr_resp.external_ip,
        external_port: map_resp.external_port,
        lease_secs: map_resp.lifetime_secs,
        protocol: NatProtocol::NatPmp,
    })
}

/// Builds an SSDP `M-SEARCH` request targeting `urn:schemas-upnp-org:device:InternetGatewayDevice:1`.
pub fn build_ssdp_msearch() -> String {
    build_ssdp_msearch_for("urn:schemas-upnp-org:device:InternetGatewayDevice:1")
}

/// Builds an SSDP `M-SEARCH` request for an arbitrary UPnP `ST` target.
pub fn build_ssdp_msearch_for(search_target: &str) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: {SSDP_MULTICAST_ADDR}:{SSDP_PORT}\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: {search_target}\r\n\
         \r\n"
    )
}

/// Extracts the `LOCATION` header URL from an SSDP `M-SEARCH` response (case-insensitive).
pub fn parse_ssdp_location(response: &str) -> Option<String> {
    for line in response.lines() {
        let line = line.trim_end_matches('\r');
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("location") {
                let url = value.trim();
                if !url.is_empty() {
                    return Some(url.to_owned());
                }
            }
        }
    }
    None
}

/// Extracts `(service_type, control_url)` for `WANIPConnection` or `WANPPPConnection` from a
/// UPnP InternetGatewayDevice description XML document.
pub fn parse_igd_control_url(description_xml: &str) -> Option<(String, String)> {
    const TARGET_SERVICES: [&str; 4] = [
        "urn:schemas-upnp-org:service:WANIPConnection:2",
        "urn:schemas-upnp-org:service:WANIPConnection:1",
        "urn:schemas-upnp-org:service:WANPPPConnection:1",
        "urn:schemas-upnp-org:service:WANPPPConnection:2",
    ];

    let mut search_from = 0usize;
    while let Some(rel_start) =
        find_ascii_case_insensitive(&description_xml[search_from..], "<service")
    {
        let block_start = search_from + rel_start;
        // Skip `<serviceList` etc.
        let after_tag = description_xml[block_start + "<service".len()..]
            .chars()
            .next();
        if !matches!(after_tag, Some(c) if c == '>' || c.is_ascii_whitespace()) {
            search_from = block_start + "<service".len();
            continue;
        }
        let Some(rel_end) =
            find_ascii_case_insensitive(&description_xml[block_start..], "</service>")
        else {
            break;
        };
        let block = &description_xml[block_start..block_start + rel_end + "</service>".len()];
        search_from = block_start + rel_end + "</service>".len();

        let Some(service_type) = extract_xml_tag_text(block, "serviceType") else {
            continue;
        };
        let is_target = TARGET_SERVICES
            .iter()
            .any(|s| service_type.eq_ignore_ascii_case(s))
            || service_type.contains("WANIPConnection")
            || service_type.contains("WANPPPConnection");
        if !is_target {
            continue;
        }
        if let Some(control_url) = extract_xml_tag_text(block, "controlURL") {
            let trimmed_st = service_type.trim();
            let trimmed_url = control_url.trim();
            if !trimmed_st.is_empty() && !trimmed_url.is_empty() {
                return Some((trimmed_st.to_owned(), trimmed_url.to_owned()));
            }
        }
    }
    None
}

/// Builds a UPnP SOAP XML `AddPortMapping` envelope body.
pub fn build_add_port_mapping_soap(
    service_type: &str,
    external_port: u16,
    internal_port: u16,
    internal_client: Ipv4Addr,
    lease_secs: u32,
    description: &str,
) -> String {
    let escaped_desc = xml_escape(description);
    format!(
        "<?xml version=\"1.0\"?>\r\n\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\r\n\
         <s:Body>\r\n\
         <u:AddPortMapping xmlns:u=\"{service_type}\">\r\n\
         <NewRemoteHost></NewRemoteHost>\r\n\
         <NewExternalPort>{external_port}</NewExternalPort>\r\n\
         <NewProtocol>TCP</NewProtocol>\r\n\
         <NewInternalPort>{internal_port}</NewInternalPort>\r\n\
         <NewInternalClient>{internal_client}</NewInternalClient>\r\n\
         <NewEnabled>1</NewEnabled>\r\n\
         <NewPortMappingDescription>{escaped_desc}</NewPortMappingDescription>\r\n\
         <NewLeaseDuration>{lease_secs}</NewLeaseDuration>\r\n\
         </u:AddPortMapping>\r\n\
         </s:Body>\r\n\
         </s:Envelope>\r\n"
    )
}

/// Builds a UPnP SOAP XML `GetExternalIPAddress` envelope body.
pub fn build_get_external_ip_soap(service_type: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\r\n\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\r\n\
         <s:Body>\r\n\
         <u:GetExternalIPAddress xmlns:u=\"{service_type}\"/>\r\n\
         </s:Body>\r\n\
         </s:Envelope>\r\n"
    )
}

/// Extracts `<NewExternalIPAddress>...</NewExternalIPAddress>` from a UPnP SOAP response.
pub fn parse_external_ip_soap(xml: &str) -> Option<Ipv4Addr> {
    let text = extract_xml_tag_text(xml, "NewExternalIPAddress")?;
    text.trim().parse::<Ipv4Addr>().ok()
}

/// Attempts UPnP IGD port mapping first, then falls back to NAT-PMP against inferred local
/// gateway candidates.
pub fn discover_and_map_tcp(
    internal_port: u16,
    lease_secs: u32,
    timeout: Duration,
) -> io::Result<NatMapping> {
    let local_ip = detect_local_ipv4().unwrap_or(Ipv4Addr::LOCALHOST);
    let upnp_err = match discover_and_map_upnp(internal_port, local_ip, lease_secs, timeout) {
        Ok(mapping) => return Ok(mapping),
        Err(e) => e,
    };

    if let Some(gw) = infer_default_gateway(local_ip) {
        if let Ok(mapping) = natpmp_map_tcp(gw, internal_port, internal_port, lease_secs, timeout) {
            return Ok(mapping);
        }
    }

    Err(upnp_err)
}

fn discover_and_map_upnp(
    internal_port: u16,
    internal_client: Ipv4Addr,
    lease_secs: u32,
    timeout: Duration,
) -> io::Result<NatMapping> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_read_timeout(Some(timeout))?;
    sock.set_write_timeout(Some(timeout))?;

    let msearch_target = SocketAddr::V4(SocketAddrV4::new(SSDP_MULTICAST_ADDR, SSDP_PORT));
    for st in [
        "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
        "urn:schemas-upnp-org:service:WANIPConnection:1",
        "urn:schemas-upnp-org:service:WANPPPConnection:1",
    ] {
        let _ = sock.send_to(build_ssdp_msearch_for(st).as_bytes(), msearch_target);
    }

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 4096];
    let mut locations: Vec<String> = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let _ = sock.set_read_timeout(Some(remaining.max(Duration::from_millis(25))));
        match sock.recv_from(&mut buf) {
            Ok((n, _)) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                if let Some(loc) = parse_ssdp_location(&text) {
                    if !locations.contains(&loc) {
                        locations.push(loc);
                    }
                }
            }
            Err(_) => break,
        }
    }

    if locations.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "no UPnP IGD SSDP response received",
        ));
    }

    let mut last_err = io::Error::other("UPnP IGD mapping failed");
    for location in locations {
        match map_via_igd_location(
            &location,
            internal_port,
            internal_client,
            lease_secs,
            timeout,
        ) {
            Ok(mapping) => return Ok(mapping),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn map_via_igd_location(
    location_url: &str,
    internal_port: u16,
    internal_client: Ipv4Addr,
    lease_secs: u32,
    timeout: Duration,
) -> io::Result<NatMapping> {
    let (host_port, desc_path) = parse_http_url(location_url).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid SSDP LOCATION URL: {location_url}"),
        )
    })?;

    let desc_resp = http_plain_request(&host_port, "GET", &desc_path, &[], None, timeout)?;
    if desc_resp.0 >= 400 {
        return Err(io::Error::other(format!(
            "UPnP description HTTP {}",
            desc_resp.0
        )));
    }
    let (service_type, control_url) = parse_igd_control_url(&desc_resp.1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "no WANIPConnection / WANPPPConnection controlURL in IGD XML",
        )
    })?;
    let (ctrl_host_port, ctrl_path) = resolve_control_url(&host_port, &desc_path, &control_url)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid UPnP controlURL: {control_url}"),
            )
        })?;

    let add_soap = build_add_port_mapping_soap(
        &service_type,
        internal_port,
        internal_port,
        internal_client,
        lease_secs,
        "syncthing-vita",
    );
    let soap_action_add = format!("\"{service_type}#AddPortMapping\"");
    let (add_status, _) = http_plain_request(
        &ctrl_host_port,
        "POST",
        &ctrl_path,
        &[
            ("Content-Type", "text/xml; charset=\"utf-8\""),
            ("SOAPAction", &soap_action_add),
        ],
        Some(&add_soap),
        timeout,
    )?;
    if add_status >= 400 {
        return Err(io::Error::other(format!(
            "UPnP AddPortMapping HTTP {add_status}"
        )));
    }

    let ip_soap = build_get_external_ip_soap(&service_type);
    let soap_action_ip = format!("\"{service_type}#GetExternalIPAddress\"");
    let (ip_status, ip_body) = http_plain_request(
        &ctrl_host_port,
        "POST",
        &ctrl_path,
        &[
            ("Content-Type", "text/xml; charset=\"utf-8\""),
            ("SOAPAction", &soap_action_ip),
        ],
        Some(&ip_soap),
        timeout,
    )?;
    if ip_status >= 400 {
        return Err(io::Error::other(format!(
            "UPnP GetExternalIPAddress HTTP {ip_status}"
        )));
    }
    let external_ip = parse_external_ip_soap(&ip_body).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing NewExternalIPAddress in SOAP response",
        )
    })?;

    Ok(NatMapping {
        external_ip,
        external_port: internal_port,
        lease_secs,
        protocol: NatProtocol::UpnpIgd,
    })
}

fn http_plain_request(
    host_port: &str,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
    body: Option<&str>,
    timeout: Duration,
) -> io::Result<(u16, String)> {
    let addr: SocketAddr = host_port.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected IP:port in UPnP URL, got {host_port}"),
        )
    })?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nUser-Agent: syncthing-core\r\nConnection: close\r\n"
    );
    for (k, v) in extra_headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        req.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    req.push_str("\r\n");
    if let Some(b) = body {
        req.push_str(b);
    }
    stream.write_all(req.as_bytes())?;
    stream.flush()?;

    let mut raw = Vec::with_capacity(4096);
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() >= MAX_HTTP_RESPONSE_BYTES {
                    break;
                }
            }
            Err(_) if !raw.is_empty() => break,
            Err(e) => return Err(e),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let parsed = super::http::parse_http_response(&text)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid UPnP HTTP response"))?;
    Ok((parsed.status, parsed.body))
}

fn parse_http_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_owned()),
    };
    let host_port = if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:80")
    };
    Some((host_port, path))
}

fn resolve_control_url(
    base_host_port: &str,
    base_path: &str,
    control_url: &str,
) -> Option<(String, String)> {
    if control_url.starts_with("http://") {
        return parse_http_url(control_url);
    }
    let path = if control_url.starts_with('/') {
        control_url.to_owned()
    } else {
        let dir = base_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        format!("{dir}/{control_url}")
    };
    Some((base_host_port.to_owned(), path))
}

fn infer_default_gateway(local_ip: Ipv4Addr) -> Option<Ipv4Addr> {
    if local_ip.is_unspecified() || local_ip.is_loopback() {
        return None;
    }
    let [a, b, c, _] = local_ip.octets();
    Some(Ipv4Addr::new(a, b, c, 1))
}

fn extract_xml_tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open_prefix = format!("<{tag}");
    let open_pos = find_ascii_case_insensitive(xml, &open_prefix)?;
    let after_name = &xml[open_pos + open_prefix.len()..];
    // Handle `<tag>` or `<tag attr="...">` (not `<tagOther>`), plus namespace prefixes via fallback.
    if after_name.starts_with("/>") {
        return Some("");
    }
    let first_ch = after_name.chars().next()?;
    if first_ch != '>' && !first_ch.is_ascii_whitespace() {
        return None;
    }
    let gt_rel = after_name.find('>')?;
    let content_start = open_pos + open_prefix.len() + gt_rel + 1;
    let close_tag = format!("</{tag}>");
    let close_rel = find_ascii_case_insensitive(&xml[content_start..], &close_tag)?;
    Some(&xml[content_start..content_start + close_rel])
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return Some(0);
    }
    h.windows(n.len()).position(|w| w.eq_ignore_ascii_case(n))
}

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}
