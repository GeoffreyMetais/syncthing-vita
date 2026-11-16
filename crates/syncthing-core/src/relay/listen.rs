//! Relay "permanent" mode: a device that cannot accept incoming TCP connections (NAT) joins a
//! relay and waits for `SessionInvitation`s, then joins each invited session (see
//! [`super::client::join_relay_session`]). The relay URL is announced on Global Discovery so
//! that peers request sessions with this device through it.

use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::wire::{
    encode_join_relay_request, encode_ping, encode_pong, read_relay_message,
    read_relay_message_after_header, RelayMessage, SessionInvitation, RELAY_ALPN,
};
use crate::cert::DeviceIdentity;
use crate::device_id::DeviceId;
use crate::tls::client::open_tls_stream_with_alpn;
use crate::tls::polling::is_socket_timeout_or_would_block;
use crate::tls::BepTlsStream;

/// We ping the relay when it has been silent this long (relays ping every minute).
const IDLE_PING_AFTER: Duration = Duration::from_secs(90);
/// The connection is considered dead when the relay has been silent this long.
const RELAY_SILENCE_TIMEOUT: Duration = Duration::from_secs(150);
/// Read timeout once the first byte of a message arrived.
const MESSAGE_COMPLETION_TIMEOUT: Duration = Duration::from_secs(10);

/// A permanent connection to a relay, receiving invitations for incoming sessions.
pub struct RelayListener {
    stream: BepTlsStream,
    relay_addr: SocketAddr,
    relay_device_id: DeviceId,
    last_heard: Instant,
    pinged: bool,
}

impl RelayListener {
    /// Connects to `relay_addr` (authenticating `relay_device_id`) and joins it permanently.
    pub fn join(
        identity: &DeviceIdentity,
        relay_addr: SocketAddr,
        relay_device_id: DeviceId,
        timeout: Duration,
    ) -> io::Result<Self> {
        let (mut stream, _) = open_tls_stream_with_alpn(
            identity,
            relay_addr,
            RELAY_ALPN,
            Some(relay_device_id),
            timeout,
        )?;
        stream.write_all(&encode_join_relay_request())?;
        stream.flush()?;
        match read_relay_message(&mut stream)? {
            RelayMessage::Response(resp) if resp.code == 0 => {}
            RelayMessage::Response(resp) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!(
                        "Relay refused JoinRelayRequest ({}): {}",
                        resp.code, resp.message
                    ),
                ))
            }
            RelayMessage::RelayFull => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    "Relay is full",
                ))
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unexpected JoinRelayRequest response: {other:?}"),
                ))
            }
        }
        Ok(Self {
            stream,
            relay_addr,
            relay_device_id,
            last_heard: Instant::now(),
            pinged: false,
        })
    }

    pub fn relay_addr(&self) -> SocketAddr {
        self.relay_addr
    }

    /// The `relay://` URI peers use to reach us through this relay (announce it on Global
    /// Discovery).
    pub fn relay_uri(&self) -> String {
        format!(
            "relay://{}/?id={}",
            self.relay_addr,
            self.relay_device_id.to_syncthing_string()
        )
    }

    /// Waits up to `poll` for an invitation, answering the relay's `Ping`s meanwhile.
    /// `Ok(None)`: nothing yet. An error means the relay connection is lost (join again).
    pub fn poll_invitation(&mut self, poll: Duration) -> io::Result<Option<SessionInvitation>> {
        let deadline = Instant::now() + poll;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            match self.try_read_message(remaining)? {
                None => self.keep_alive()?,
                Some(message) => {
                    self.last_heard = Instant::now();
                    self.pinged = false;
                    match message {
                        RelayMessage::Ping => {
                            self.stream.write_all(&encode_pong())?;
                            self.stream.flush()?;
                        }
                        RelayMessage::SessionInvitation(inv) => return Ok(Some(inv)),
                        RelayMessage::Pong | RelayMessage::Response(_) => {}
                        RelayMessage::RelayFull => {
                            return Err(io::Error::new(
                                io::ErrorKind::ConnectionAborted,
                                "Relay is full",
                            ))
                        }
                    }
                }
            }
        }
    }

    /// Pings a silent relay once, and gives up on one silent for too long.
    fn keep_alive(&mut self) -> io::Result<()> {
        let silence = self.last_heard.elapsed();
        if silence >= RELAY_SILENCE_TIMEOUT {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Relay connection silent",
            ));
        }
        if silence >= IDLE_PING_AFTER && !self.pinged {
            self.pinged = true;
            self.stream.write_all(&encode_ping())?;
            self.stream.flush()?;
        }
        Ok(())
    }

    /// Reads one message if its first byte arrives within `poll`.
    fn try_read_message(&mut self, poll: Duration) -> io::Result<Option<RelayMessage>> {
        let _ = self.stream.get_mut().set_read_timeout(Some(poll));
        let mut header = [0u8; 12];
        loop {
            match self.stream.read(&mut header[..1]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Relay closed the connection",
                    ))
                }
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if is_socket_timeout_or_would_block(&e) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
        let _ = self
            .stream
            .get_mut()
            .set_read_timeout(Some(MESSAGE_COMPLETION_TIMEOUT));
        self.stream.read_exact(&mut header[1..])?;
        read_relay_message_after_header(&mut self.stream, header).map(Some)
    }
}
