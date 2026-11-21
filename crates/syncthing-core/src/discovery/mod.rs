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
pub mod nat;
pub mod udp;

pub use addresses::{is_local_ip, resolve_peer_tcp_addrs};
pub use announce::{Announce, DISCOVERY_BROADCAST_ADDR, DISCOVERY_MAGIC, DISCOVERY_PORT};
pub use global::{
    lookup_global_peer, lookup_peer_on, parse_global_discovery_json, resolve_endpoint_uri,
    PeerEndpoint, DEFAULT_LOOKUP_SERVER,
};
pub use global_announce::{announce_on, AnnounceOutcome, DEFAULT_ANNOUNCE_SERVERS};
pub use lan_scan::scan_lan_bep_peers;
pub use local_ip::detect_local_ipv4;
pub use nat::{
    build_add_port_mapping_soap, build_get_external_ip_soap, build_ssdp_msearch,
    decode_natpmp_map_resp, decode_natpmp_public_addr_resp, discover_and_map_tcp,
    encode_natpmp_map_req, encode_natpmp_public_addr_req, natpmp_map_tcp, natpmp_map_tcp_at,
    parse_external_ip_soap, parse_igd_control_url, parse_ssdp_location, NatMapping,
    NatPmpMapResponse, NatPmpPublicAddrResponse, NatProtocol, NATPMP_PORT, SSDP_MULTICAST_ADDR,
    SSDP_PORT,
};
pub use udp::{run_discovery_cycle, DiscoveryCycleResult};
