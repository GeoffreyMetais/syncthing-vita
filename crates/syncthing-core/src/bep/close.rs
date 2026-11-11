//! `Close` message (type 7).

use std::io;

use crate::proto::{encode_bytes_field, WireIter};

/// `Close` message (type 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepClose {
    pub reason: String,
}

impl BepClose {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(2 + self.reason.len());
        if !self.reason.is_empty() {
            encode_bytes_field(&mut buf, 1, self.reason.as_bytes());
        }
        buf
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut reason = String::new();
        for item in WireIter::new(payload) {
            let (field_num, wire_type, slice) = item?;
            if field_num == 1 && wire_type == 2 {
                reason = String::from_utf8_lossy(slice).into_owned();
            }
        }
        Ok(Self { reason })
    }
}
