//! Post-`Hello` BEP frame reading and writing (`[u16 header len][Header][u32 msg len][payload]`).

use std::io::{self, Read, Write};

use super::header::{BepCompression, BepHeader, BepMessageType};
use super::lz4::decompress_bep_lz4;

/// Writes an uncompressed post-`Hello` BEP message to a stream as a single contiguous frame.
pub fn write_bep_message<W: Write>(
    stream: &mut W,
    msg_type: BepMessageType,
    payload: &[u8],
) -> io::Result<()> {
    let hdr = BepHeader {
        msg_type,
        compression: BepCompression::None,
    };
    let hdr_bytes = hdr.encode();
    let hdr_len = hdr_bytes.len() as u16;
    let msg_len = payload.len() as u32;

    let mut frame = Vec::with_capacity(2 + hdr_bytes.len() + 4 + payload.len());
    frame.extend_from_slice(&hdr_len.to_be_bytes());
    frame.extend_from_slice(&hdr_bytes);
    frame.extend_from_slice(&msg_len.to_be_bytes());
    frame.extend_from_slice(payload);

    stream.write_all(&frame)?;
    stream.flush()
}

/// Reads and decompresses (if LZ4) the next post-`Hello` BEP message from a stream.
pub fn read_bep_message<R: Read>(stream: &mut R) -> io::Result<(BepMessageType, Vec<u8>)> {
    let mut first = [0u8; 1];
    stream.read_exact(&mut first)?;
    read_bep_message_after_first_byte(stream, first[0])
}

/// Completes reading and decompressing a post-`Hello` BEP message after its first header-length byte (`first_byte`) has been read.
pub fn read_bep_message_after_first_byte<R: Read>(
    stream: &mut R,
    first_byte: u8,
) -> io::Result<(BepMessageType, Vec<u8>)> {
    let mut second = [0u8; 1];
    stream.read_exact(&mut second)?;
    let hdr_len = u16::from_be_bytes([first_byte, second[0]]) as usize;

    let mut hdr_buf = vec![0u8; hdr_len];
    stream.read_exact(&mut hdr_buf)?;
    let header = BepHeader::decode(&hdr_buf)?;

    let mut msg_len_buf = [0u8; 4];
    stream.read_exact(&mut msg_len_buf)?;
    let msg_len = u32::from_be_bytes(msg_len_buf) as usize;

    if msg_len > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "BEP frame exceeds Vita RAM safeguard limit: {} bytes",
                msg_len
            ),
        ));
    }

    let mut raw_msg = vec![0u8; msg_len];
    stream.read_exact(&mut raw_msg)?;

    let payload = match header.compression {
        BepCompression::None => raw_msg,
        BepCompression::Lz4 => decompress_bep_lz4(&raw_msg)?,
    };

    Ok((header.msg_type, payload))
}
