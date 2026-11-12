//! Download side: pulls a peer's newer files block by block (128 KiB delta sync with streaming
//! disk writes) over the primary connection, while still answering the peer's own requests.

use std::io;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::context::SessionContext;
use super::events::SyncEvent;
use super::upload::serve_block_request;
use crate::bep::{
    read_bep_message, write_bep_message, BepBlockInfo, BepFileInfo, BepMessageType, BepRequest,
    BepResponse,
};
use crate::tls::BepTlsStream;

/// Generous per-response timeout: servers on NAS / Docker hosts may take several seconds
/// to read a block from spinning disks before answering.
const BLOCK_RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);

/// Brings `folder_id` up to date with the peer's `files`.
/// Directories, deletions and identical content are handled locally (version-vector plan);
/// only files with a newer remote version are downloaded.
/// Returns `true` if anything changed locally.
pub fn download_folder(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folder_id: &str,
    files: &[BepFileInfo],
    count_up_to_date: bool,
) -> io::Result<bool> {
    if ctx.storage.is_folder_paused(folder_id)
        || !ctx
            .storage
            .is_folder_shared_with(folder_id, &ctx.peer_device_id)
    {
        return Ok(false);
    }

    let prev_timeout = stream.get_ref().read_timeout().ok().flatten();
    let _ = stream
        .get_mut()
        .set_read_timeout(Some(BLOCK_RESPONSE_TIMEOUT));
    let result = apply_pull_plan(stream, ctx, folder_id, files, count_up_to_date);
    let _ = stream.get_mut().set_read_timeout(prev_timeout);
    result
}

fn apply_pull_plan(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folder_id: &str,
    files: &[BepFileInfo],
    count_up_to_date: bool,
) -> io::Result<bool> {
    let plan = match ctx.storage.plan_pull(folder_id, ctx.own_id(), files) {
        Ok(p) => p,
        Err(e) => {
            // Local folder problem (missing / unreadable root): skip this folder only, the
            // session and the other folders keep running.
            ctx.emit(SyncEvent::FolderUnavailable {
                folder: folder_id.to_owned(),
                error: e.to_string(),
            });
            return Ok(false);
        }
    };

    let mut any_changed = false;
    for line in &plan.applied {
        ctx.emit(SyncEvent::LocalChangeApplied {
            folder: folder_id.to_owned(),
            description: line.clone(),
        });
        any_changed = true;
    }
    if count_up_to_date {
        for file in &plan.up_to_date {
            ctx.stats.files_up_to_date += 1;
            ctx.emit(SyncEvent::FileUpToDate {
                folder: folder_id.to_owned(),
                name: file.name.clone(),
                size: file.size.max(0) as u64,
            });
        }
    }
    for file in &plan.downloads {
        if download_file(stream, ctx, folder_id, file)? {
            any_changed = true;
        }
    }
    Ok(any_changed)
}

/// Downloads the outdated blocks of one file into a temp file and commits it.
/// Local filesystem errors only skip this file (`Ok(false)`); protocol errors are returned.
fn download_file(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folder_id: &str,
    file: &BepFileInfo,
) -> io::Result<bool> {
    let storage = ctx.storage;
    let outdated_blocks = match storage
        .find_outdated_blocks(folder_id, file)
        .and_then(|b| storage.prepare_download(folder_id, &file.name).map(|_| b))
    {
        Ok(b) => b,
        Err(e) => {
            emit_file_error(ctx, folder_id, file, &e);
            return Ok(false);
        }
    };

    let mut file_bytes = 0usize;
    let mut file_blocks = 0usize;
    for block in &outdated_blocks {
        if ctx.is_stop_requested() {
            storage.abort_download(folder_id, &file.name);
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "stopped locally during a download",
            ));
        }
        match fetch_block(stream, ctx, folder_id, file, block) {
            Ok(Some(len)) => {
                file_blocks += 1;
                file_bytes += len;
            }
            Ok(None) => {
                storage.abort_download(folder_id, &file.name);
                return Ok(false);
            }
            Err(e) => {
                storage.abort_download(folder_id, &file.name);
                return Err(e);
            }
        }
    }

    let total_size = file.size.max(0) as u64;
    if let Err(e) = storage.commit_download(folder_id, &file.name, total_size) {
        storage.abort_download(folder_id, &file.name);
        emit_file_error(ctx, folder_id, file, &e);
        return Ok(false);
    }
    storage.record_downloaded(folder_id, ctx.own_id(), file);
    ctx.stats.files_downloaded += 1;
    ctx.emit(SyncEvent::FileSynced {
        folder: folder_id.to_owned(),
        name: file.name.clone(),
        blocks_downloaded: file_blocks,
        bytes_downloaded: file_bytes,
        total_size,
    });
    Ok(true)
}

/// Requests one block and waits for its `Response`, serving peer `Request`s and answering
/// `Ping`s meanwhile. Returns the stored length, or `None` if the file must be skipped.
fn fetch_block(
    stream: &mut BepTlsStream,
    ctx: &mut SessionContext<'_>,
    folder_id: &str,
    file: &BepFileInfo,
    block: &BepBlockInfo,
) -> io::Result<Option<usize>> {
    let req_id = ctx.next_request_id();
    let req = BepRequest {
        id: req_id,
        folder: folder_id.to_owned(),
        name: file.name.clone(),
        offset: block.offset,
        size: block.size,
        hash: block.hash.clone(),
    };
    write_bep_message(stream, BepMessageType::Request, &req.encode())?;

    let resp = loop {
        let (mtype, payload) = read_bep_message(stream)?;
        match mtype {
            BepMessageType::Response => {
                let resp = BepResponse::decode(&payload)?;
                if resp.id == req_id {
                    break resp;
                }
            }
            BepMessageType::Request => serve_block_request(stream, ctx, &payload)?,
            BepMessageType::Ping => write_bep_message(stream, BepMessageType::Ping, &[])?,
            _ => {}
        }
    };

    if resp.code != 0 {
        return Ok(None);
    }
    if resp.data.len() != block.size.max(0) as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Block of {} bytes received for {} ({} expected)",
                resp.data.len(),
                file.name,
                block.size
            ),
        ));
    }
    if block.hash.len() == 32 && Sha256::digest(&resp.data).as_slice() != block.hash.as_slice() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid SHA-256 block hash for {}", file.name),
        ));
    }
    let offset = block.offset.max(0) as u64;
    let total_size = file.size.max(0) as u64;
    if let Err(e) = ctx
        .storage
        .write_block_streaming(folder_id, &file.name, offset, &resp.data, total_size)
    {
        // Local filesystem error (invalid name on exFAT, path clash, ...):
        // skip this file but keep the BEP session alive.
        emit_file_error(ctx, folder_id, file, &e);
        return Ok(None);
    }

    let len = resp.data.len();
    ctx.stats.bytes_downloaded += len;
    ctx.emit(SyncEvent::BlockDownloaded {
        folder: folder_id.to_owned(),
        name: file.name.clone(),
        offset,
        size: len,
        total_size,
    });
    Ok(Some(len))
}

fn emit_file_error(
    ctx: &mut SessionContext<'_>,
    folder_id: &str,
    file: &BepFileInfo,
    error: &io::Error,
) {
    ctx.emit(SyncEvent::FileError {
        folder: folder_id.to_owned(),
        name: file.name.clone(),
        error: error.to_string(),
    });
}
