//! Every tunable value of the background sync cycle, in one place.
//!
//! The worker runs inside `SceShell`, next to the LiveArea, with very little memory and no user
//! watching: everything it does is bounded in time (a cycle ends on its own) and in memory
//! (one peer session at a time, a single connection, a fixed heap on the PS Vita).

use std::time::Duration;

/// Device name announced to peers: the same as the app's, so peers see a single device.
pub const DEVICE_NAME: &str = "PS-Vita";
/// Client name announced to peers (`Hello`), as the app does.
pub const CLIENT_NAME: &str = "vita-syncthing";

/// Data directory of the app, shared with the worker (`devices.cfg`, `shares.cfg`, ...).
pub const VITA_DATA_DIR: &str = "ux0:data/syncthing";

/// Connections per peer session announced in our `Hello`. The app opens `1+2`; the worker
/// keeps a single TLS connection (each one holds about 100 KiB of TLS buffers).
pub const WORKER_CONNECTIONS: i32 = 1;
/// Maximum number of devices synchronized in one cycle (one after the other).
pub const MAX_PEERS_PER_CYCLE: usize = 4;

/// TCP connect / TLS handshake / blocking socket timeout when dialing a device.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// How long the cycle's single LAN discovery round listens for announcements.
pub const DISCOVERY_LISTEN: Duration = Duration::from_secs(2);
/// Timeout of a Global Discovery lookup and of a relay handshake.
pub const GLOBAL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(6);

/// Silence of a peer after which its session ends, once our index is published. Syncthing
/// starts pulling our changes within a second or two of receiving them.
pub const CYCLE_DRAIN_IDLE: Duration = Duration::from_secs(6);
/// Upper bound of that drain, whatever the peer does.
pub const CYCLE_DRAIN_MAX: Duration = Duration::from_secs(120);
/// Hard limit of a whole cycle: the worker asks its session to stop once it is reached.
pub const CYCLE_MAX_DURATION: Duration = Duration::from_secs(15 * 60);
/// Interval of the keepalive `Ping`s (scans of large folders included).
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);
/// Interval at which the abort signal and the cycle deadline are checked.
pub const ABORT_POLL: Duration = Duration::from_millis(200);

/// Stack of the thread running the cycle and its peer sessions (TLS handshakes, scans).
/// `tests/stack_budget.rs` runs a whole cycle of the unoptimized host build in a thread
/// of this size.
pub const CYCLE_THREAD_STACK: usize = 512 * 1024;
/// Stack of the watcher thread turning the abort signal into a stop request.
pub const WATCHER_THREAD_STACK: usize = 64 * 1024;

/// Size above which `syncthing.log` is rotated to `syncthing.log.1` (the app's
/// `LOG_MAX_BYTES`: both write the same file).
pub const LOG_MAX_BYTES: u64 = 512 * 1024;
/// Prefix of every line of `syncthing.log` (the app's prefix: its log viewer strips it).
pub const LOG_LINE_PREFIX: &str = "[VITA-SYNCTHING] ";
/// Marks the worker's lines in the shared log.
pub const LOG_TAG: &str = "Background sync: ";
