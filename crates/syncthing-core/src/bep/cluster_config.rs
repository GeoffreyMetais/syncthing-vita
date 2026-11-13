//! `ClusterConfig` message (type 0): shared folders and per-device index state.

use std::io;

use crate::device_id::DeviceId;
use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

/// Shared folder in `ClusterConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepFolder {
    pub id: String,
    pub label: String,
    pub device_ids: Vec<DeviceId>,
}

/// Folder type announced in `ClusterConfig` (BEP `Folder.type`, field 3).
///
/// Only the types the Vita supports locally are modelled. The wire values are Syncthing's
/// (`SEND_RECEIVE = 0`, `RECEIVE_ONLY = 2`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FolderType {
    /// Local changes are sent and remote changes are applied (read-write share).
    #[default]
    SendReceive,
    /// Remote changes are applied, local changes are never sent (read-only share).
    ReceiveOnly,
}

impl FolderType {
    /// BEP wire value of the folder type.
    pub fn wire_value(self) -> u64 {
        match self {
            Self::SendReceive => 0,
            Self::ReceiveOnly => 2,
        }
    }
}

/// Folder index state metadata for `ClusterConfig`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FolderIndexState {
    pub index_id: u64,
    pub max_sequence: i64,
    pub paused: bool,
    pub folder_type: FolderType,
}

/// `ClusterConfig` message (type 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepClusterConfig {
    pub folders: Vec<BepFolder>,
    pub secondary: bool,
}

impl BepClusterConfig {
    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        let mut folders = Vec::new();
        let mut secondary = false;
        for item in WireIter::new(payload) {
            let (field_num, wire_type, data) = item?;
            match (field_num, wire_type) {
                (1, 2) => folders.push(Self::decode_folder(data)?),
                (2, 0) => {
                    let (v, _) = decode_varint(data)?;
                    secondary = v != 0;
                }
                _ => {}
            }
        }
        Ok(Self { folders, secondary })
    }

    /// Encodes a `ClusterConfig` for a secondary BEP data connection (`secondary = true`).
    pub fn encode_secondary() -> Vec<u8> {
        let mut out = Vec::with_capacity(4);
        encode_varint_field(&mut out, 2, 1);
        out
    }

    fn decode_folder(bytes: &[u8]) -> io::Result<BepFolder> {
        let mut id = String::new();
        let mut label = String::new();
        let mut device_ids = Vec::new();

        for item in WireIter::new(bytes) {
            let (field_num, wire_type, data) = item?;
            if wire_type == 2 {
                match field_num {
                    1 => id = String::from_utf8_lossy(data).into_owned(),
                    2 => label = String::from_utf8_lossy(data).into_owned(),
                    16 => {
                        for dev_item in WireIter::new(data) {
                            let (df, dw, ddata) = dev_item?;
                            if df == 1 && dw == 2 && ddata.len() == 32 {
                                let mut raw = [0u8; 32];
                                raw.copy_from_slice(ddata);
                                device_ids.push(DeviceId(raw));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(BepFolder {
            id,
            label,
            device_ids,
        })
    }

    /// Encodes a `ClusterConfig` advertising our shared folders, `index_id`, and `max_sequence`.
    pub fn encode_for_peers(
        folders: &[(BepFolder, FolderIndexState)],
        own_id: DeviceId,
        peer_id: DeviceId,
    ) -> Vec<u8> {
        let mut out = Vec::new();

        for (folder, state) in folders {
            let mut folder_buf = Vec::new();
            encode_bytes_field(&mut folder_buf, 1, folder.id.as_bytes());
            if !folder.label.is_empty() {
                encode_bytes_field(&mut folder_buf, 2, folder.label.as_bytes());
            }
            if state.folder_type != FolderType::SendReceive {
                encode_varint_field(&mut folder_buf, 3, state.folder_type.wire_value());
            }
            if state.paused {
                encode_varint_field(&mut folder_buf, 7, 1);
            }

            // Device entry for PS Vita (with index_id and max_sequence from local index)
            // BEP v1 Device fields: 1=id, 2=name, 3=addresses, 4=compression, 6=max_sequence, 8=index_id
            {
                let mut dev_buf = Vec::new();
                encode_bytes_field(&mut dev_buf, 1, &own_id.0);
                encode_bytes_field(&mut dev_buf, 2, b"PS-Vita");
                encode_bytes_field(&mut dev_buf, 3, b"dynamic");
                encode_varint_field(&mut dev_buf, 4, 1); // COMPRESSION_NEVER
                if state.max_sequence > 0 {
                    encode_varint_field(&mut dev_buf, 6, state.max_sequence as u64);
                }
                if state.index_id != 0 {
                    encode_varint_field(&mut dev_buf, 8, state.index_id);
                }
                encode_bytes_field(&mut folder_buf, 16, &dev_buf);
            }

            // Device entry for remote peer
            {
                let mut dev_buf = Vec::new();
                encode_bytes_field(&mut dev_buf, 1, &peer_id.0);
                encode_bytes_field(&mut dev_buf, 3, b"dynamic");
                encode_varint_field(&mut dev_buf, 4, 1); // COMPRESSION_NEVER
                encode_bytes_field(&mut folder_buf, 16, &dev_buf);
            }

            encode_bytes_field(&mut out, 1, &folder_buf);
        }

        out
    }
}
