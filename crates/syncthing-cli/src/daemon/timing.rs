//! Every timing and limit of the daemon, in one place.

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

/// Incoming connections handled at the same time that are not a configured device's session
/// yet (TLS handshake in progress, or an unknown device being turned away). Above it, the
/// listener closes new connections at once.
pub const MAX_UNAUTHENTICATED_CONNECTIONS: usize = 32;
/// Relay invitations from configured devices handled at the same time that are not a running
/// session yet (relay session being joined, TLS handshake in progress). Above it, a new
/// invitation is dropped at once, without a thread. Separate from the listener's connections,
/// so that a relay (a third party) cannot keep directly connecting devices out, nor the reverse.
pub const MAX_PENDING_RELAY_INVITATIONS: usize = 16;
/// Unknown devices (LAN announcements, connections, relay invitations) reported individually;
/// beyond it, one line says that no more are reported.
pub const MAX_UNKNOWN_DEVICE_REPORTS: usize = 256;
/// Distinct reports kept per subject (a configured device's LAN addresses, folder offers,
/// lookup errors; a discovery server's announcements): beyond it, that subject is quiet.
pub const MAX_REPORTS_PER_SUBJECT: usize = 32;
/// Unknown devices whose last direct LAN answer is remembered (one entry each, dropped after
/// the reply interval): beyond it, other unknown devices are not answered until entries expire.
pub const MAX_LAN_REPLIES_TO_UNKNOWN: usize = 64;
