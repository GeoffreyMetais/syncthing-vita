//! BEP/1.0 post-`Hello` message header: message type and compression mode.

use std::io;

use crate::proto::{decode_varint, encode_varint_field, WireIter};

/// BEP/1.0 message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BepMessageType {
    ClusterConfig = 0,
    Index = 1,
    IndexUpdate = 2,
    Request = 3,
    Response = 4,
    DownloadProgress = 5,
    Ping = 6,
    Close = 7,
    Unknown(u8),
}

impl From<u64> for BepMessageType {
    fn from(v: u64) -> Self {
        match v {
            0 => Self::ClusterConfig,
            1 => Self::Index,
            2 => Self::IndexUpdate,
            3 => Self::Request,
            4 => Self::Response,
            5 => Self::DownloadProgress,
            6 => Self::Ping,
            7 => Self::Close,
            other => Self::Unknown(other as u8),
        }
    }
}

impl BepMessageType {
    pub fn as_u64(self) -> u64 {
        match self {
            Self::ClusterConfig => 0,
            Self::Index => 1,
            Self::IndexUpdate => 2,
            Self::Request => 3,
            Self::Response => 4,
            Self::DownloadProgress => 5,
            Self::Ping => 6,
            Self::Close => 7,
            Self::Unknown(v) => v as u64,
        }
    }
}

/// Compression mode of a BEP frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BepCompression {
    None = 0,
    Lz4 = 1,
}

/// Protobuf header (`Header`) preceding each post-`Hello` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BepHeader {
    pub msg_type: BepMessageType,
    pub compression: BepCompression,
}

impl BepHeader {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8);
        if self.msg_type.as_u64() != 0 {
            encode_varint_field(&mut buf, 1, self.msg_type.as_u64());
        }
        if self.compression == BepCompression::Lz4 {
            encode_varint_field(&mut buf, 2, 1);
        }
        buf
    }

    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut msg_type = BepMessageType::ClusterConfig;
        let mut compression = BepCompression::None;

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            if wire_type == 0 {
                let (v, _) = decode_varint(data)?;
                match field_num {
                    1 => msg_type = BepMessageType::from(v),
                    2 => {
                        compression = if v == 1 {
                            BepCompression::Lz4
                        } else {
                            BepCompression::None
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(Self {
            msg_type,
            compression,
        })
    }
}
