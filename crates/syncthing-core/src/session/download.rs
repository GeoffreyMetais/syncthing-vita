//! Download side: pulls a peer's newer files block by block (128 KiB delta sync with streaming
//! disk writes) over the primary connection, while still answering the peer's own requests.

use std::fs::File;
use std::io;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::context::SessionContext;
use super::events::SyncEvent;
use super::keepalive::send_keepalive_if_due;
use super::upload::serve_block_request;
use crate::bep::{
    read_bep_message, write_bep_message, BepBlockInfo, BepFileInfo, BepMessageType, BepRequest,
    BepResponse,
};
use crate::storage::download::write_block_at;
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
    let storage = ctx.storage;
    let own_id = ctx.own_id();
    let planned = storage.plan_pull_with_progress(folder_id, own_id, files, &mut || {
        send_keepalive_if_due(stream, ctx)
    });
    let plan = match planned {
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
    // Completed files are recorded in the database as they come, saved at intervals; whatever
    // ends the downloads (success, protocol error, stop request), the rest is saved here.
    let mut skipped = false;
    let downloads = plan.downloads.iter().try_for_each(|file| {
        if download_file(stream, ctx, folder_id, file)? {
            any_changed = true;
        } else {
            skipped = true;
        }
        Ok(())
    });
    storage.flush_folder_db(folder_id);
    if skipped {
        ctx.mark_folder_incomplete(folder_id);
    }
    downloads.map(|()| any_changed)
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
    // The local content is only worth copying when some of its blocks are kept.
    let prepared = storage
        .find_outdated_blocks(folder_id, file)
        .and_then(|outdated| {
            let reuse_local = outdated.len() < file.blocks.len();
            storage.prepare_download_seeded(folder_id, &file.name, reuse_local)?;
            let tmp = storage.open_download(folder_id, &file.name)?;
            Ok((outdated, tmp))
        });
    let (outdated_blocks, mut tmp) = match prepared {
        Ok(p) => p,
        Err(e) => {
            storage.abort_download(folder_id, &file.name);
            emit_file_error(ctx, folder_id, file, &e);
            return Ok(false);
        }
    };

    // Fetches every outdated block: `Ok(None)` if the file must be skipped.
    let mut fetch_all = || -> io::Result<Option<(usize, usize)>> {
        let (mut blocks, mut bytes) = (0usize, 0usize);
        for block in &outdated_blocks {
            if ctx.is_stop_requested() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "stopped locally during a download",
                ));
            }
            match fetch_block(stream, ctx, folder_id, file, block, &mut tmp)? {
                Some(len) => {
                    blocks += 1;
                    bytes += len;
                }
                None => return Ok(None),
            }
        }
        Ok(Some((blocks, bytes)))
    };
    let fetched = fetch_all();
    // The temp file must be closed before it is removed or renamed into place.
    drop(tmp);
    let (file_blocks, file_bytes) = match fetched {
        Ok(Some(counts)) => counts,
        Ok(None) => {
            storage.abort_download(folder_id, &file.name);
            return Ok(false);
        }
        Err(e) => {
            storage.abort_download(folder_id, &file.name);
            return Err(e);
        }
    };

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
        bytes_downloaded: file_bytes as u64,
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
    tmp: &mut File,
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
            // Index changes, a new `ClusterConfig` or `Close` cannot be applied in the middle
            // of a file: keep them for the session loop instead of losing them.
            BepMessageType::Index
            | BepMessageType::IndexUpdate
            | BepMessageType::ClusterConfig
            | BepMessageType::Close => ctx.defer_message(mtype, payload),
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
    if let Err(e) = write_block_at(tmp, offset, &resp.data) {
        // Local filesystem error (invalid name on exFAT, path clash, ...):
        // skip this file but keep the BEP session alive.
        emit_file_error(ctx, folder_id, file, &e);
        return Ok(None);
    }

    let len = resp.data.len();
    ctx.stats.bytes_downloaded += len as u64;
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
