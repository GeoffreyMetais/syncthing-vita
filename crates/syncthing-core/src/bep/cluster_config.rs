//! `ClusterConfig` message (type 0): shared folders and per-device index state.

use std::io;

use crate::device_id::DeviceId;
use crate::proto::{decode_varint, encode_bytes_field, encode_varint_field, WireIter};

/// Compression preference for a device entry in `ClusterConfig` (`Device.compression`, field 4).
///
/// Matches Syncthing BEP v1 `enum Compression`: `METADATA = 0`, `NEVER = 1`, `ALWAYS = 2`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BepDeviceCompression {
    /// Compress metadata messages (`Index`, `IndexUpdate`, `ClusterConfig`, etc.).
    #[default]
    Metadata = 0,
    /// Disable compression for all messages.
    Never = 1,
    /// Compress all messages including `Response` block data.
    Always = 2,
}

impl BepDeviceCompression {
    /// Decodes a wire varint into `BepDeviceCompression`, defaulting to `Metadata` for unknown values.
    pub fn from_wire(v: u64) -> Self {
        match v {
            0 => Self::Metadata,
            1 => Self::Never,
            2 => Self::Always,
            _ => Self::Metadata,
        }
    }

    /// BEP wire value of this compression preference.
    pub fn wire_value(self) -> u64 {
        self as u64
    }
}

/// Shared folder in `ClusterConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BepFolder {
    pub id: String,
    pub label: String,
    pub device_ids: Vec<DeviceId>,
}

/// Folder type announced in `ClusterConfig` (BEP `Folder.type`, field 3).
///
/// Matches Syncthing's `FolderType` enum:
/// (`SEND_RECEIVE = 0`, `SEND_ONLY = 1`, `RECEIVE_ONLY = 2`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FolderType {
    /// Local changes are sent and remote changes are applied (read-write share).
    #[default]
    SendReceive,
    /// Local changes are sent, remote changes are never applied locally (send-only share).
    SendOnly,
    /// Remote changes are applied, local changes are never sent (read-only share).
    ReceiveOnly,
}

impl FolderType {
    /// BEP wire value of the folder type.
    pub fn wire_value(self) -> u64 {
        match self {
            Self::SendReceive => 0,
            Self::SendOnly => 1,
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

    /// Decodes the `compression` preference advertised by `peer_id` across all folders in `payload`
    /// (BEP `Device.compression`, field 4; defaults to [`BepDeviceCompression::Metadata`]).
    pub fn decode_peer_compression(payload: &[u8], peer_id: &DeviceId) -> BepDeviceCompression {
        for item in WireIter::new(payload) {
            let Ok((field_num, wire_type, data)) = item else {
                break;
            };
            if field_num != 1 || wire_type != 2 {
                continue;
            }
            for f_item in WireIter::new(data) {
                let Ok((ff, fw, fdata)) = f_item else {
                    break;
                };
                if ff != 16 || fw != 2 {
                    continue;
                }
                let mut dev_id: Option<DeviceId> = None;
                let mut compression = BepDeviceCompression::Metadata;
                for d_item in WireIter::new(fdata) {
                    let Ok((df, dw, ddata)) = d_item else {
                        break;
                    };
                    match (df, dw) {
                        (1, 2) if ddata.len() == 32 => {
                            let mut raw = [0u8; 32];
                            raw.copy_from_slice(ddata);
                            dev_id = Some(DeviceId(raw));
                        }
                        (4, 0) => {
                            if let Ok((v, _)) = decode_varint(ddata) {
                                compression = BepDeviceCompression::from_wire(v);
                            }
                        }
                        _ => {}
                    }
                }
                if dev_id.is_some_and(|id| id.matches_peer(peer_id)) {
                    return compression;
                }
            }
        }
        BepDeviceCompression::Metadata
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

    /// Encodes a `ClusterConfig` advertising our shared folders, `index_id`, and `max_sequence`,
    /// using [`BepDeviceCompression::Metadata`] for device entries.
    pub fn encode_for_peers(
        folders: &[(BepFolder, FolderIndexState)],
        own_id: DeviceId,
        own_name: &str,
        peer_id: DeviceId,
    ) -> Vec<u8> {
        Self::encode_for_peers_with_compression(
            folders,
            own_id,
            own_name,
            peer_id,
            BepDeviceCompression::Metadata,
        )
    }

    /// Encodes a `ClusterConfig` advertising our shared folders, `index_id`, `max_sequence`,
    /// and the specified `compression` preference on device entries.
    pub fn encode_for_peers_with_compression(
        folders: &[(BepFolder, FolderIndexState)],
        own_id: DeviceId,
        own_name: &str,
        peer_id: DeviceId,
        compression: BepDeviceCompression,
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

            // Our own device entry (with index_id and max_sequence from local index)
            // BEP v1 Device fields: 1=id, 2=name, 3=addresses, 4=compression, 6=max_sequence, 8=index_id
            {
                let mut dev_buf = Vec::new();
                encode_bytes_field(&mut dev_buf, 1, &own_id.0);
                if !own_name.is_empty() {
                    encode_bytes_field(&mut dev_buf, 2, own_name.as_bytes());
                }
                encode_bytes_field(&mut dev_buf, 3, b"dynamic");
                if compression != BepDeviceCompression::Metadata {
                    encode_varint_field(&mut dev_buf, 4, compression.wire_value());
                }
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
                if compression != BepDeviceCompression::Metadata {
                    encode_varint_field(&mut dev_buf, 4, compression.wire_value());
                }
                encode_bytes_field(&mut folder_buf, 16, &dev_buf);
            }

            encode_bytes_field(&mut out, 1, &folder_buf);
        }

        out
    }
}
