//! Every timing of the daemon, in one place.

use std::time::Duration;

/// TCP connect, TLS handshake and blocking socket operations of a session.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Keepalive `Ping` interval of established sessions.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// How often a stopping daemon's threads look at the stop flag.
pub const STOP_CHECK_STEP: Duration = Duration::from_millis(200);

/// Delay before dialing a device again after a failed attempt (plus up to `RETRY_JITTER`,
/// so that two devices dialing each other do not keep colliding).
pub const RETRY_AFTER_ERROR: Duration = Duration::from_secs(10);
/// Delay before dialing a device again after its session ended.
pub const RETRY_AFTER_SESSION: Duration = Duration::from_secs(3);
/// Delay before dialing again a device that did not accept us yet.
pub const RETRY_AWAITING_PAIRING: Duration = Duration::from_secs(30);
pub const RETRY_JITTER: Duration = Duration::from_millis(2500);
/// Poll interval of a dialer that knows no address for its device yet.
pub const NO_ADDRESS_POLL: Duration = Duration::from_secs(2);
/// Poll interval of a dialer whose device is connected (through another link).
pub const CONNECTED_POLL: Duration = Duration::from_secs(1);

/// Local Discovery announcement interval (Syncthing: 30 s).
pub const LAN_ANNOUNCE_INTERVAL: Duration = Duration::from_secs(30);
/// Minimum delay between two direct answers to the same device's announcements.
pub const LAN_REPLY_INTERVAL: Duration = Duration::from_secs(5);
/// Read timeout of the Local Discovery socket.
pub const LAN_RECV_TIMEOUT: Duration = Duration::from_millis(500);

/// Global Discovery lookup interval of a device that is not connected.
pub const GLOBAL_LOOKUP_INTERVAL: Duration = Duration::from_secs(60);
/// Timeout of a Global Discovery / relay pool HTTPS request.
pub const HTTPS_TIMEOUT: Duration = Duration::from_secs(15);

/// Poll interval of a relay connection waiting for invitations.
pub const RELAY_POLL: Duration = Duration::from_secs(1);
/// Delay before trying the next relay after losing one.
pub const RELAY_RETRY: Duration = Duration::from_secs(20);
/// Delay before fetching the relay pool again after a failure.
pub const RELAY_POOL_RETRY: Duration = Duration::from_secs(300);

/// `--exit-when-synced`: no transfer for this long after the last completed sync cycle.
pub const SYNCED_QUIET_PERIOD: Duration = Duration::from_secs(3);
