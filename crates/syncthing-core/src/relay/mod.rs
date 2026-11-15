//! Syncthing Relay Protocol v1 client (`relay://` connections through NAT): dialing a peer
//! through a relay ([`client`]), being reachable through one ([`listen`]), the public relay
//! list ([`pool`]).

pub mod client;
pub mod listen;
pub mod pool;
pub mod wire;

pub use client::{connect_via_relay, join_relay_session, request_relay_session};
pub use listen::RelayListener;
pub use pool::{fetch_relay_pool, parse_relay_pool_json, DEFAULT_RELAY_POOL};
pub use wire::{RelayMessage, RelayResponse, SessionInvitation};
