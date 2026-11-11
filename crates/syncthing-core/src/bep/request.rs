//! Block transfer messages: `Request` (type 3) and `Response` (type 4).

use std::io;

use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

/// `Request` message (type 3) to request or serve a file block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepRequest {
    pub id: i32,
    pub folder: String,
    pub name: String,
    pub offset: i64,
    pub size: i32,
    pub hash: Vec<u8>,
}

impl BepRequest {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        encode_varint_field(&mut buf, 1, self.id as u64);
        encode_bytes_field(&mut buf, 2, self.folder.as_bytes());
        encode_bytes_field(&mut buf, 3, self.name.as_bytes());
        if self.offset != 0 {
            encode_varint_field(&mut buf, 4, self.offset as u64);
        }
        if self.size != 0 {
            encode_varint_field(&mut buf, 5, self.size as u64);
        }
        if !self.hash.is_empty() {
            encode_bytes_field(&mut buf, 6, &self.hash);
        }
        buf
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut id = 0i32;
        let mut folder = String::new();
        let mut name = String::new();
        let mut offset = 0i64;
        let mut size = 0i32;
        let mut hash = Vec::new();

        for item in WireIter::new(payload) {
            let (field_num, wire_type, slice) = item?;
            match (field_num, wire_type) {
                (1, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    id = v as i32;
                }
                (2, 2) => folder = String::from_utf8_lossy(slice).into_owned(),
                (3, 2) => name = String::from_utf8_lossy(slice).into_owned(),
                (4, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    offset = v as i64;
                }
                (5, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    size = v as i32;
                }
                (6, 2) => hash = slice.to_vec(),
                _ => {}
            }
        }

        Ok(Self {
            id,
            folder,
            name,
            offset,
            size,
            hash,
        })
    }
}

/// `Response` message (type 4) containing block data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepResponse {
    pub id: i32,
    pub data: Vec<u8>,
    pub code: i32,
}

impl BepResponse {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.data.len());
        encode_varint_field(&mut buf, 1, self.id as u64);
        if !self.data.is_empty() {
            encode_bytes_field(&mut buf, 2, &self.data);
        }
        if self.code != 0 {
            encode_varint_field(&mut buf, 3, self.code as u64);
        }
        buf
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut id = 0i32;
        let mut data = Vec::new();
        let mut code = 0i32;

        for item in WireIter::new(payload) {
            let (field_num, wire_type, slice) = item?;
            match (field_num, wire_type) {
                (1, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    id = v as i32;
                }
                (2, 2) => data = slice.to_vec(),
                (3, 0) => {
                    let (v, _) = decode_varint(slice)?;
                    code = v as i32;
                }
                _ => {}
            }
        }

        Ok(Self { id, data, code })
    }
}
