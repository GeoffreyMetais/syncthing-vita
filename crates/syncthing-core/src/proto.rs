//! Minimalist Protobuf v3 encoder / decoder without external dependencies (`protoc`),
//! designed for Syncthing Local Discovery v4 (`Announce`) and BEP (`Hello`, `Header`) messages.

use std::io;

/// Encodes a `u64` integer into Protobuf Varint format.
pub fn encode_varint(mut val: u64, buf: &mut Vec<u8>) {
    while val >= 0x80 {
        buf.push(((val & 0x7F) as u8) | 0x80);
        val >>= 7;
    }
    buf.push(val as u8);
}

/// Decodes a Protobuf Varint and returns `(value, bytes_read)`.
pub fn decode_varint(bytes: &[u8]) -> io::Result<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;

    for (i, &b) in bytes.iter().enumerate() {
        if shift >= 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Protobuf Varint exceeds 64 bits",
            ));
        }
        result |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Ok((result, i + 1));
        }
        shift += 7;
    }

    Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "Truncated Protobuf Varint",
    ))
}

/// Encodes a `bytes` or `string` field (wire type 2 = Length-delimited).
pub fn encode_bytes_field(buf: &mut Vec<u8>, field_number: u32, data: &[u8]) {
    let tag = ((field_number as u64) << 3) | 2;
    encode_varint(tag, buf);
    encode_varint(data.len() as u64, buf);
    buf.extend_from_slice(data);
}

/// Encodes an `int64` / `uint64` / `int32` / `enum` field (wire type 0 = Varint).
pub fn encode_varint_field(buf: &mut Vec<u8>, field_number: u32, val: u64) {
    let tag = (field_number as u64) << 3;
    encode_varint(tag, buf);
    encode_varint(val, buf);
}

/// Zero-copy iterator over Protobuf message fields.
/// Returns `(field_number, wire_type, field_slice)`.
pub struct WireIter<'a> {
    rest: &'a [u8],
}

impl<'a> WireIter<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { rest: buf }
    }
}

impl<'a> Iterator for WireIter<'a> {
    type Item = io::Result<(u32, u8, &'a [u8])>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }

        let (tag, tag_len) = match decode_varint(self.rest) {
            Ok(v) => v,
            Err(e) => {
                self.rest = &[];
                return Some(Err(e));
            }
        };
        self.rest = &self.rest[tag_len..];

        let field_number = (tag >> 3) as u32;
        let wire_type = (tag & 0x07) as u8;

        match wire_type {
            0 => {
                // Varint: slice exact varint bytes
                let (_, val_len) = match decode_varint(self.rest) {
                    Ok(v) => v,
                    Err(e) => {
                        self.rest = &[];
                        return Some(Err(e));
                    }
                };
                let slice = &self.rest[..val_len];
                self.rest = &self.rest[val_len..];
                Some(Ok((field_number, wire_type, slice)))
            }
            1 => {
                // 64-bit
                if self.rest.len() < 8 {
                    self.rest = &[];
                    return Some(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Truncated 64-bit field",
                    )));
                }
                let slice = &self.rest[..8];
                self.rest = &self.rest[8..];
                Some(Ok((field_number, wire_type, slice)))
            }
            2 => {
                // Length-delimited
                let (len, len_bytes) = match decode_varint(self.rest) {
                    Ok(v) => v,
                    Err(e) => {
                        self.rest = &[];
                        return Some(Err(e));
                    }
                };
                self.rest = &self.rest[len_bytes..];
                let len = len as usize;
                if self.rest.len() < len {
                    self.rest = &[];
                    return Some(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Truncated length-delimited field",
                    )));
                }
                let slice = &self.rest[..len];
                self.rest = &self.rest[len..];
                Some(Ok((field_number, wire_type, slice)))
            }
            5 => {
                // 32-bit
                if self.rest.len() < 4 {
                    self.rest = &[];
                    return Some(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Truncated 32-bit field",
                    )));
                }
                let slice = &self.rest[..4];
                self.rest = &self.rest[4..];
                Some(Ok((field_number, wire_type, slice)))
            }
            other => {
                self.rest = &[];
                Some(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unsupported Protobuf wire type: {other}"),
                )))
            }
        }
    }
}
