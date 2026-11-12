//! Upload side: serves block `Request`s from peers out of local PS Vita storage.

use std::io::{self, Write};

use super::context::SessionContext;
use super::events::SyncEvent;
use crate::bep::{write_bep_message, BepMessageType, BepRequest, BepResponse};

/// BEP `ErrorCode::NO_ERROR`.
const CODE_NO_ERROR: i32 = 0;
/// BEP `ErrorCode::GENERIC`.
const CODE_GENERIC: i32 = 1;
/// BEP `ErrorCode::NO_SUCH_FILE`.
const CODE_NO_SUCH_FILE: i32 = 2;

/// Answers an incoming `BepRequest` with the requested block (or `NO_SUCH_FILE`).
/// A folder that is paused or not shared with this peer is refused with `GENERIC`.
pub fn serve_block_request<W: Write>(
    stream: &mut W,
    ctx: &mut SessionContext<'_>,
    payload: &[u8],
) -> io::Result<()> {
    let req = BepRequest::decode(payload)?;
    if ctx.storage.is_folder_paused(&req.folder)
        || !ctx
            .storage
            .is_folder_shared_with(&req.folder, &ctx.peer_device_id)
    {
        let resp = BepResponse {
            id: req.id,
            data: Vec::new(),
            code: CODE_GENERIC,
        };
        return write_bep_message(stream, BepMessageType::Response, &resp.encode());
    }
    let offset = req.offset.max(0) as u64;
    match ctx
        .storage
        .read_block_for_peer(&req.folder, &req.name, offset, req.size.max(0) as usize)
    {
        Ok(data) => {
            let len = data.len();
            let resp = BepResponse {
                id: req.id,
                data,
                code: CODE_NO_ERROR,
            };
            write_bep_message(stream, BepMessageType::Response, &resp.encode())?;
            ctx.stats.blocks_uploaded += 1;
            ctx.stats.bytes_uploaded += len;
            ctx.emit(SyncEvent::BlockUploaded {
                folder: req.folder,
                name: req.name,
                offset,
                size: len,
            });
        }
        Err(_) => {
            let resp = BepResponse {
                id: req.id,
                data: Vec::new(),
                code: CODE_NO_SUCH_FILE,
            };
            write_bep_message(stream, BepMessageType::Response, &resp.encode())?;
        }
    }
    Ok(())
}
