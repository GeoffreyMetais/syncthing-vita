//! Peer discovery: Local Discovery v4 (UDP `21027`) and the LAN TCP-scan fallback.
//!
//! - [`announce`] — `Announce` packet codec and protocol constants (pure, no I/O)
//! - [`addresses`] — ordering of advertised TCP addresses (pure, no I/O)
//! - [`local_ip`] — local IPv4 detection
//! - [`udp`] — UDP broadcast / listen cycle
//! - [`lan_scan`] — TCP `22000` subnet scan + TLS probe fallback

pub mod addresses;
pub mod announce;
pub mod lan_scan;
pub mod local_ip;
pub mod udp;

pub use addresses::resolve_peer_tcp_addrs;
pub use announce::{Announce, DISCOVERY_BROADCAST_ADDR, DISCOVERY_MAGIC, DISCOVERY_PORT};
pub use lan_scan::scan_lan_bep_peers;
pub use local_ip::detect_local_ipv4;
pub use udp::{run_discovery_cycle, DiscoveryCycleResult};
