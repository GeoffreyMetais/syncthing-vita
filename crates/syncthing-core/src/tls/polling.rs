//! Non-blocking polling of BEP frames on a TLS stream.

use std::io::{self, Read};
use std::time::Duration;

use super::BepTlsStream;
use crate::bep::{read_bep_message_after_first_byte, BepMessageType};

/// Read timeout used once the first byte of a frame has arrived.
const FRAME_COMPLETION_TIMEOUT: Duration = Duration::from_secs(10);

/// Returns `true` if `e` represents a non-fatal socket read timeout or would-block condition
/// on standard Unix or PlayStation Vita (`SceNet` / Newlib) sockets.
pub fn is_socket_timeout_or_would_block(e: &io::Error) -> bool {
    if matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    ) {
        return true;
    }
    if let Some(code) = e.raw_os_error() {
        let u = code as u32;
        if matches!(code, 4 | 11 | 35 | 60 | 116) || u == 0x8041_0123 || u == 0x8041_013C {
            return true;
        }
    }
    false
}

/// Polls a TLS BEP stream for the start of a frame within `poll_timeout`.
/// - If no byte arrives within `poll_timeout`, returns `Ok(None)` without consuming any bytes.
/// - As soon as the first byte of a BEP frame arrives, raises the read timeout to `10s` to read
///   the complete frame so a multi-packet frame is never truncated mid-read over PS Vita Wi-Fi.
pub fn try_read_bep_message(
    stream: &mut BepTlsStream,
    poll_timeout: Duration,
) -> io::Result<Option<(BepMessageType, Vec<u8>)>> {
    let _ = stream.get_mut().set_read_timeout(Some(poll_timeout));
    let mut first = [0u8; 1];
    loop {
        match stream.read(&mut first) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "BEP peer closed stream",
                ));
            }
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if is_socket_timeout_or_would_block(&e) => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    let _ = stream
        .get_mut()
        .set_read_timeout(Some(FRAME_COMPLETION_TIMEOUT));
    let res = read_bep_message_after_first_byte(stream, first[0]);
    let _ = stream.get_mut().set_read_timeout(Some(poll_timeout));
    res.map(Some)
}
