//! PS Vita filesystem helpers (`sceIo*` quirks): `device:` root spelling, tolerant `mkdir`
//! and recursive listing that skips Syncthing-internal files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// `ux0:music` -> `ux0:/music` (the form `sceIo*` always accepts), or `None` if the path is not
/// of the `device:relative` form.
pub fn with_device_root_slash(path: &Path) -> Option<PathBuf> {
    let s = path.to_str()?;
    let (device, rest) = s.split_once(':')?;
    let valid_device = !device.is_empty()
        && !device.contains('/')
        && device.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid_device || rest.is_empty() || rest.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(format!("{}:/{}", device, rest)))
}

/// On PS Vita, `stat()` / `opendir()` on a top-level `device:dir` path (e.g. `ux0:music`) can fail
/// although the directory exists. Prefer whichever spelling can actually be listed.
pub(crate) fn accessible_root(path: PathBuf) -> PathBuf {
    if fs::read_dir(&path).is_ok() {
        return path;
    }
    match with_device_root_slash(&path) {
        Some(alt) if fs::read_dir(&alt).is_ok() => alt,
        _ => path,
    }
}

/// Creates `dir` (and its parents) if needed.
///
/// On PS Vita, `sceIoMkdir` reports `EEXIST` for existing directories and `std::fs::create_dir_all`
/// then relies on `stat()` to confirm it is a directory, which can fail for some paths (e.g. names
/// normalized by exFAT). Treat the directory as present whenever it can actually be listed.
pub(crate) fn ensure_dir_exists(dir: &Path) -> io::Result<()> {
    match fs::create_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(_) if fs::read_dir(dir).is_ok() => Ok(()),
        Err(e) => Err(io::Error::new(
            e.kind(),
            format!("mkdir {}: {e}", dir.display()),
        )),
    }
}

/// Recursively lists `(relative path, is_dir)` under `base`, skipping Syncthing-internal files.
pub(crate) fn collect_files_recursive(
    base: &Path,
    prefix: &str,
    out: &mut Vec<(String, bool)>,
) -> io::Result<()> {
    for entry_res in fs::read_dir(base)? {
        let entry = entry_res?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Internal files only: download temp files, local databases and Syncthing markers.
        if name.starts_with(".syncthing") || name.starts_with(".st") {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{}/{}", prefix, name)
        };
        let path = entry.path();
        if path.is_dir() {
            out.push((rel.clone(), true));
            collect_files_recursive(&path, &rel, out)?;
        } else if path.is_file() {
            out.push((rel, false));
        }
    }
    Ok(())
}
