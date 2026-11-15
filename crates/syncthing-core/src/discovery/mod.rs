//! Peer discovery: Local Discovery v4 (UDP `21027`), the LAN TCP-scan fallback and Global
//! Discovery v2 (HTTPS).
//!
//! - [`announce`] — `Announce` packet codec and protocol constants (pure, no I/O)
//! - [`addresses`] — ordering of advertised TCP addresses (pure, no I/O)
//! - [`local_ip`] — local IPv4 detection
//! - [`udp`] — UDP broadcast / listen cycle
//! - [`lan_scan`] — TCP `22000` subnet scan + TLS probe fallback
//! - [`http`] — minimal HTTP/1.1 client for the Global Discovery / relay pool servers
//! - [`global`] — Global Discovery v2 lookups; [`global_announce`] — announcements

pub mod addresses;
pub mod announce;
pub mod global;
pub mod global_announce;
pub mod http;
pub mod lan_scan;
pub mod local_ip;
pub mod udp;

pub use addresses::resolve_peer_tcp_addrs;
pub use announce::{Announce, DISCOVERY_BROADCAST_ADDR, DISCOVERY_MAGIC, DISCOVERY_PORT};
pub use global::{
    lookup_global_peer, lookup_peer_on, parse_global_discovery_json, resolve_endpoint_uri,
    PeerEndpoint, DEFAULT_LOOKUP_SERVER,
};
pub use global_announce::{announce_on, AnnounceOutcome, DEFAULT_ANNOUNCE_SERVERS};
pub use lan_scan::scan_lan_bep_peers;
pub use local_ip::detect_local_ipv4;
pub use udp::{run_discovery_cycle, DiscoveryCycleResult};
