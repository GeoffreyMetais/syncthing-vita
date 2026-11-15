//! Local Discovery v4 (UDP 21027): broadcasts our announcement periodically, listens
//! continuously, and answers a device's announcement directly (the PS Vita app only listens
//! for a moment after its own broadcast).

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use syncthing_core::{
    detect_local_ipv4, resolve_peer_tcp_addrs, Announce, DeviceId, DISCOVERY_PORT,
};

use super::context::DaemonContext;
use super::log::log;
use super::timing::{LAN_ANNOUNCE_INTERVAL, LAN_RECV_TIMEOUT, LAN_REPLY_INTERVAL};

/// A device heard on the LAN.
#[derive(Debug, Clone)]
pub struct LanPeer {
    pub id: DeviceId,
    /// Where the announcement came from.
    pub source: SocketAddr,
    /// Its TCP addresses (unspecified IPs replaced with the source IP).
    pub addrs: Vec<SocketAddr>,
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

/// The Local Discovery socket and its schedule.
pub struct LanBeacon {
    socket: UdpSocket,
    announce: Announce,
    last_broadcast: Option<Instant>,
    /// Last direct answer per device.
    replies: Vec<(DeviceId, Instant)>,
}

impl LanBeacon {
    /// Binds UDP 21027. When another program holds it (e.g. Syncthing on the same computer),
    /// falls back to a random port: our broadcasts still go out and direct answers still come
    /// back, but other devices' broadcasts are missed.
    pub fn bind(announce: Announce) -> io::Result<Self> {
        let socket = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) {
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
            replies: Vec::new(),
        })
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
        let due = self
            .replies
            .iter()
            .find(|(id, _)| *id == peer.id)
            .is_none_or(|(_, at)| at.elapsed() >= LAN_ANNOUNCE_INTERVAL.max(LAN_REPLY_INTERVAL));
        if due {
            let _ = self
                .socket
                .send_to(&self.announce.encode_packet(), peer.source);
            self.replies.retain(|(id, _)| *id != peer.id);
            self.replies.push((peer.id, Instant::now()));
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
    while !ctx.is_stopping() {
        let Some(peer) = beacon.step() else {
            continue;
        };
        match ctx.peer(&peer.id) {
            Some(known) => {
                if ctx.first_report(format!("lan:{}:{:?}", peer.id, peer.addrs)) {
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
                if ctx.first_report(format!("lan-unknown:{}", peer.id)) {
                    log(&format!(
                        "LAN: unknown device {} at {}. To add it: syncthing-cli device add {}",
                        peer.id,
                        peer.source.ip(),
                        peer.id
                    ));
                }
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
