//! Local Discovery v4 (UDP 21027): broadcasts our announcement periodically, listens
//! continuously, and answers a device's announcement directly (the PS Vita app only listens
//! for a moment after its own broadcast).

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use syncthing_core::{
    detect_local_ipv4, resolve_peer_tcp_addrs, Announce, DeviceId, DISCOVERY_PORT,
};

use super::context::DaemonContext;
use super::log::log;
use super::timing::{
    LAN_ANNOUNCE_INTERVAL, LAN_RECV_TIMEOUT, LAN_REPLY_INTERVAL, MAX_LAN_REPLIES_TO_UNKNOWN,
};

/// A device heard on the LAN.
#[derive(Debug, Clone)]
pub struct LanPeer {
    pub id: DeviceId,
    /// Where the announcement came from.
    pub source: SocketAddr,
    /// Its TCP addresses (unspecified IPs replaced with the source IP).
    pub addrs: Vec<SocketAddr>,
}

/// Binds `0.0.0.0:port` with `SO_REUSEADDR` and `SO_REUSEPORT`: every socket bound this way
/// on the computer receives the broadcasts sent to the port.
fn bind_shared(port: u16) -> io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&SockAddr::from(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        port,
    )))?;
    Ok(socket.into())
}

/// Our announcement: `tcp://0.0.0.0:<port>` when we listen (peers substitute the address the
/// packet came from), nothing otherwise.
pub fn our_announce(id: DeviceId, listen_port: Option<u16>) -> Announce {
    Announce {
        id,
        addresses: listen_port
            .map(|p| vec![format!("tcp://0.0.0.0:{p}")])
            .unwrap_or_default(),
        instance_id: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(1),
    }
}

/// Minimum delay between two direct answers to the same device.
fn reply_interval() -> Duration {
    LAN_ANNOUNCE_INTERVAL.max(LAN_REPLY_INTERVAL)
}

/// When to answer an announcement directly: at most once per reply interval per device. Only
/// the answers of the last interval are remembered: every configured device, and at most
/// [`MAX_LAN_REPLIES_TO_UNKNOWN`] other devices (announcements are not authenticated: anyone
/// can announce any number of IDs), so configured devices are always answered.
#[derive(Debug, Default)]
pub struct ReplySchedule {
    configured: Vec<DeviceId>,
    /// Last direct answer per device, within the reply interval.
    last: Vec<(DeviceId, Instant)>,
}

impl ReplySchedule {
    pub fn new(configured: Vec<DeviceId>) -> Self {
        Self {
            configured,
            last: Vec::new(),
        }
    }

    /// `true` if `id`'s announcement heard at `now` must be answered (the answer is recorded).
    pub fn is_due(&mut self, id: DeviceId, now: Instant) -> bool {
        let interval = reply_interval();
        self.last
            .retain(|(_, at)| now.saturating_duration_since(*at) < interval);
        if self.last.iter().any(|(d, _)| *d == id) {
            return false;
        }
        if !self.configured.contains(&id) {
            let unknown = self
                .last
                .iter()
                .filter(|(d, _)| !self.configured.contains(d))
                .count();
            if unknown >= MAX_LAN_REPLIES_TO_UNKNOWN {
                return false;
            }
        }
        self.last.push((id, now));
        true
    }

    /// Number of answers remembered.
    pub fn len(&self) -> usize {
        self.last.len()
    }

    /// `true` when no answer is remembered.
    pub fn is_empty(&self) -> bool {
        self.last.is_empty()
    }
}

/// The Local Discovery socket and its schedule.
pub struct LanBeacon {
    socket: UdpSocket,
    announce: Announce,
    last_broadcast: Option<Instant>,
    replies: ReplySchedule,
}

impl LanBeacon {
    /// Binds UDP 21027, shared with the other programs listening on it (Syncthing, another
    /// instance: each one receives the broadcasts). When a program holds it exclusively,
    /// falls back to a random port: our broadcasts still go out and direct answers still come
    /// back, but other devices' broadcasts are missed.
    pub fn bind(announce: Announce) -> io::Result<Self> {
        let socket = match bind_shared(DISCOVERY_PORT) {
            Ok(s) => s,
            Err(e) => {
                log(&format!(
                    "UDP {DISCOVERY_PORT} unavailable ({e}): LAN discovery only hears direct answers"
                ));
                UdpSocket::bind(("0.0.0.0", 0))?
            }
        };
        socket.set_broadcast(true)?;
        socket.set_read_timeout(Some(LAN_RECV_TIMEOUT))?;
        Ok(Self {
            socket,
            announce,
            last_broadcast: None,
            replies: ReplySchedule::default(),
        })
    }

    /// The configured devices: always answered (see [`ReplySchedule`]).
    pub fn set_configured(&mut self, configured: Vec<DeviceId>) {
        self.replies = ReplySchedule::new(configured);
    }

    /// The UDP port actually bound (21027 unless another program holds it exclusively).
    pub fn local_port(&self) -> io::Result<u16> {
        Ok(self.socket.local_addr()?.port())
    }

    /// Broadcasts our announcement on the global and `/24` subnet broadcast addresses.
    fn broadcast(&mut self) {
        let packet = self.announce.encode_packet();
        let global = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_PORT);
        let _ = self.socket.send_to(&packet, global);
        if let Some(ip) = detect_local_ipv4() {
            let o = ip.octets();
            let subnet = SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(o[0], o[1], o[2], 255)),
                DISCOVERY_PORT,
            );
            let _ = self.socket.send_to(&packet, subnet);
        }
        self.last_broadcast = Some(Instant::now());
    }

    /// Answers `peer` directly, at most once per announcement interval.
    fn reply(&mut self, peer: &LanPeer) {
        if self.replies.is_due(peer.id, Instant::now()) {
            let _ = self
                .socket
                .send_to(&self.announce.encode_packet(), peer.source);
        }
    }

    /// Broadcasts if due, then waits up to the receive timeout for one announcement from
    /// another device (answered directly).
    pub fn step(&mut self) -> Option<LanPeer> {
        if self
            .last_broadcast
            .is_none_or(|t| t.elapsed() >= LAN_ANNOUNCE_INTERVAL)
        {
            self.broadcast();
        }
        let mut buf = [0u8; 2048];
        let (len, source) = self.socket.recv_from(&mut buf).ok()?;
        let announce = Announce::decode_packet(&buf[..len]).ok()?;
        if announce.id == self.announce.id {
            return None;
        }
        let peer = LanPeer {
            id: announce.id,
            source,
            addrs: resolve_peer_tcp_addrs(source, &announce.addresses),
        };
        self.reply(&peer);
        Some(peer)
    }
}

/// Runs Local Discovery until a stop is requested, recording the addresses of the devices
/// heard and reporting unknown ones once.
pub fn run_lan_discovery(ctx: Arc<DaemonContext>) {
    let port = *ctx.listen_port.lock().unwrap_or_else(|p| p.into_inner());
    let mut beacon = match LanBeacon::bind(our_announce(ctx.identity.device_id, port)) {
        Ok(b) => b,
        Err(e) => {
            log(&format!("LAN discovery disabled: {e}"));
            return;
        }
    };
    beacon.set_configured(ctx.peers.iter().map(|p| p.id).collect());
    while !ctx.is_stopping() {
        let Some(peer) = beacon.step() else {
            continue;
        };
        match ctx.peer(&peer.id) {
            Some(known) => {
                if ctx.first_report(&format!("lan:{}", peer.id), &format!("{:?}", peer.addrs)) {
                    log(&format!(
                        "LAN: {} announces {}",
                        known.name,
                        format_addrs(&peer.addrs, peer.source)
                    ));
                }
                if !peer.addrs.is_empty() {
                    ctx.table().set_lan_addrs(peer.id, peer.addrs);
                }
            }
            None => {
                ctx.report_unknown_device("lan", peer.id, || {
                    format!(
                        "LAN: unknown device {} at {}. To add it: syncthing-cli device add {}",
                        peer.id,
                        peer.source.ip(),
                        peer.id
                    )
                });
            }
        }
    }
}

/// Addresses for display; a device announcing none (the PS Vita app never listens) is shown
/// by its source address.
pub fn format_addrs(addrs: &[SocketAddr], source: SocketAddr) -> String {
    if addrs.is_empty() {
        format!("no address (heard from {})", source.ip())
    } else {
        addrs
            .iter()
            .map(|a| format!("tcp://{a}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
