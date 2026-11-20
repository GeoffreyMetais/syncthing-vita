//! Syncthing Relay Protocol v1 client:
//! 1. Protocol mode (`TLS`, `ALPN = "bep-relay"`): send `ConnectRequest` -> receive `SessionInvitation`.
//! 2. Session mode (plain TCP): send `JoinSessionRequest` -> receive `Response(0, "success")`.

use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use super::wire::{
    encode_connect_request, encode_join_session_request, read_relay_message, RelayMessage,
    SessionInvitation, RELAY_ALPN,
};
use crate::cert::DeviceIdentity;
use crate::device_id::DeviceId;
use crate::tls::client::open_tls_stream_with_alpn;

/// Connects to `relay_addr` (authenticating `relay_device_id` via TLS) and requests a relay
/// session to `target_device_id`.
pub fn request_relay_session(
    identity: &DeviceIdentity,
    relay_addr: SocketAddr,
    relay_device_id: DeviceId,
    target_device_id: DeviceId,
    timeout: Duration,
) -> io::Result<SessionInvitation> {
    let (mut tls, _) = open_tls_stream_with_alpn(
        identity,
        relay_addr,
        RELAY_ALPN,
        Some(relay_device_id),
        timeout,
    )?;
    let req = encode_connect_request(&target_device_id);
    tls.write_all(&req)?;
    tls.flush()?;

    match read_relay_message(&mut tls)? {
        RelayMessage::SessionInvitation(inv) => Ok(inv),
        RelayMessage::Response(resp) => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Relay rejected ConnectRequest ({}): {}",
                resp.code, resp.message
            ),
        )),
        RelayMessage::RelayFull => Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "Relay is full",
        )),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unexpected relay response: {other:?}"),
        )),
    }
}

/// Joins a relay session in plain-text session mode and returns the raw relayed `TcpStream`
/// ready for the BEP TLS 1.3 handshake.
pub fn join_relay_session(
    relay_addr: SocketAddr,
    invitation: &SessionInvitation,
    timeout: Duration,
) -> io::Result<TcpStream> {
    let session_ip = invitation.session_ip(relay_addr.ip());
    let session_port = if invitation.port == 0 {
        relay_addr.port()
    } else {
        invitation.port
    };
    let session_addr = SocketAddr::new(session_ip, session_port);
    let mut tcp = if session_addr == relay_addr {
        TcpStream::connect_timeout(&session_addr, timeout)?
    } else {
        // Bound the first attempt so a blackholed NAT port or unroutable IP still leaves
        // time to fall back to `relay_addr` before the relay's 10s invitation expires.
        let first_timeout = (timeout / 2)
            .min(Duration::from_secs(3))
            .max(Duration::from_millis(100));
        match TcpStream::connect_timeout(&session_addr, first_timeout) {
            Ok(tcp) => tcp,
            Err(_) => {
                let fallback_timeout = timeout
                    .saturating_sub(first_timeout)
                    .max(Duration::from_secs(1));
                TcpStream::connect_timeout(&relay_addr, fallback_timeout)?
            }
        }
    };
    let _ = tcp.set_nodelay(true);
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;

    let req = encode_join_session_request(&invitation.key);
    tcp.write_all(&req)?;
    tcp.flush()?;

    match read_relay_message(&mut tcp)? {
        RelayMessage::Response(resp) if resp.code == 0 => Ok(tcp),
        RelayMessage::Response(resp) => Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("Relay JoinSession failed ({}): {}", resp.code, resp.message),
        )),
        RelayMessage::RelayFull => Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "Relay session full",
        )),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unexpected JoinSession response: {other:?}"),
        )),
    }
}

/// Establishes a relayed TCP tunnel to `target_device_id` via `relay_addr` (`relay_device_id`).
pub fn connect_via_relay(
    identity: &DeviceIdentity,
    relay_addr: SocketAddr,
    relay_device_id: DeviceId,
    target_device_id: DeviceId,
    timeout: Duration,
) -> io::Result<TcpStream> {
    let inv = request_relay_session(
        identity,
        relay_addr,
        relay_device_id,
        target_device_id,
        timeout,
    )?;
    if inv.server_socket {
        return Err(io::Error::other(
            "Relay requested ServerSocket=true for initiator",
        ));
    }
    join_relay_session(relay_addr, &inv, timeout)
}
