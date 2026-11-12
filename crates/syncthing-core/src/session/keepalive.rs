//! Keepalive `Ping`s, also sent from inside long local operations (scans, hashing) so the
//! peer does not drop the connection while the session is busy.

use super::context::SessionContext;
use crate::bep::{write_bep_message, BepMessageType};
use crate::tls::BepTlsStream;

/// Sends a `Ping` on the primary connection if the keepalive interval elapsed.
/// Errors are ignored here: a dead connection is reported by the next read or write.
pub fn send_keepalive_if_due(stream: &mut BepTlsStream, ctx: &mut SessionContext<'_>) {
    if ctx.keepalive_due() {
        let _ = write_bep_message(stream, BepMessageType::Ping, &[]);
    }
}
