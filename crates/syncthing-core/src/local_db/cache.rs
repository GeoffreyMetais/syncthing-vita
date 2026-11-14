//! Process-wide cache of folder databases shared by all concurrent peer sessions,
//! with load-on-first-use and crash-safe (`tmp` + backup + rename) persistence.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::FolderDb;
use crate::device_id::DeviceId;
use crate::storage::atomic::{recover_replaced_file, replace_file, write_file_synced};

/// Longest time changes recorded with [`with_folder_db_deferred`] stay unsaved.
const DEFERRED_SAVE_INTERVAL: Duration = Duration::from_secs(5);

/// A cached database and when it was last written to disk.
struct CachedDb {
    db: FolderDb,
    saved_at: Instant,
}

fn db_cache() -> &'static Mutex<HashMap<PathBuf, CachedDb>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedDb>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// When a changed database is written back to disk.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SavePolicy {
    /// Right after the change.
    Now,
    /// At most every [`DEFERRED_SAVE_INTERVAL`]; the next `Now` access or
    /// [`flush_folder_db`] saves what is pending.
    Deferred,
}

/// Runs `f` on the folder database stored at `db_path` while holding the process-wide lock,
/// loading it on first use and persisting it afterwards if it changed.
pub fn with_folder_db<R>(
    db_path: &Path,
    folder_id: &str,
    own_id: DeviceId,
    f: impl FnOnce(&mut FolderDb) -> R,
) -> R {
    access_folder_db(db_path, folder_id, own_id, SavePolicy::Now, f)
}

/// Same as [`with_folder_db`] for small changes repeated many times in a row (one record per
/// downloaded file): the database is serialized in full on every save, so it is only written
/// every [`DEFERRED_SAVE_INTERVAL`]. Call [`flush_folder_db`] at the end of the batch.
pub fn with_folder_db_deferred<R>(
    db_path: &Path,
    folder_id: &str,
    own_id: DeviceId,
    f: impl FnOnce(&mut FolderDb) -> R,
) -> R {
    access_folder_db(db_path, folder_id, own_id, SavePolicy::Deferred, f)
}

/// Writes the cached database of `db_path` to disk if it has unsaved changes.
pub fn flush_folder_db(db_path: &Path) {
    let mut cache = db_cache().lock().unwrap_or_else(|p| p.into_inner());
    if let Some(cached) = cache.get_mut(db_path) {
        save_if_dirty(db_path, cached);
    }
}

fn access_folder_db<R>(
    db_path: &Path,
    folder_id: &str,
    own_id: DeviceId,
    policy: SavePolicy,
    f: impl FnOnce(&mut FolderDb) -> R,
) -> R {
    let mut cache = db_cache().lock().unwrap_or_else(|p| p.into_inner());
    let cached = cache.entry(db_path.to_path_buf()).or_insert_with(|| {
        recover_replaced_file(db_path, &db_path.with_extension("bak"));
        let db = fs::read_to_string(db_path)
            .ok()
            .and_then(|t| FolderDb::parse(&t))
            .unwrap_or_else(|| FolderDb::new(folder_id, own_id));
        CachedDb {
            db,
            saved_at: Instant::now(),
        }
    });
    let result = f(&mut cached.db);
    if policy == SavePolicy::Now || cached.saved_at.elapsed() >= DEFERRED_SAVE_INTERVAL {
        save_if_dirty(db_path, cached);
    }
    result
}

fn save_if_dirty(db_path: &Path, cached: &mut CachedDb) {
    if cached.db.dirty && save_db(db_path, &cached.db).is_ok() {
        cached.db.dirty = false;
        cached.saved_at = Instant::now();
    }
}

fn save_db(db_path: &Path, db: &FolderDb) -> io::Result<()> {
    if let Some(parent) = db_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = db_path.with_extension("tmp");
    write_file_synced(&tmp, db.serialize().as_bytes())?;
    replace_file(&tmp, db_path, &db_path.with_extension("bak"))
}
