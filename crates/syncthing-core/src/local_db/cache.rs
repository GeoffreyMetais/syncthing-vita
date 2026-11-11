//! Process-wide cache of folder databases shared by all concurrent peer sessions,
//! with load-on-first-use and atomic (`tmp` + rename) persistence.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::FolderDb;
use crate::device_id::DeviceId;

fn db_cache() -> &'static Mutex<HashMap<PathBuf, FolderDb>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, FolderDb>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Runs `f` on the folder database stored at `db_path` while holding the process-wide lock,
/// loading it on first use and persisting it afterwards if it changed.
pub fn with_folder_db<R>(
    db_path: &Path,
    folder_id: &str,
    own_id: DeviceId,
    f: impl FnOnce(&mut FolderDb) -> R,
) -> R {
    let mut cache = db_cache().lock().unwrap_or_else(|p| p.into_inner());
    let db = cache.entry(db_path.to_path_buf()).or_insert_with(|| {
        fs::read_to_string(db_path)
            .ok()
            .and_then(|t| FolderDb::parse(&t))
            .unwrap_or_else(|| FolderDb::new(folder_id, own_id))
    });
    let result = f(db);
    if db.dirty && save_db(db_path, db).is_ok() {
        db.dirty = false;
    }
    result
}

fn save_db(db_path: &Path, db: &FolderDb) -> io::Result<()> {
    if let Some(parent) = db_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = db_path.with_extension("tmp");
    fs::write(&tmp, db.serialize())?;
    let _ = fs::remove_file(db_path);
    fs::rename(&tmp, db_path)
}
