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

/// What a directory entry found by [`collect_files_recursive`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiskEntryKind {
    Dir,
    File,
    Symlink(String),
    /// The entry exists but cannot be inspected right now (`stat()` failed). It must never be
    /// mistaken for a deleted file.
    Unreadable,
}

/// One entry of a folder, with its path relative to the folder root.
#[derive(Debug)]
pub(crate) struct DiskEntry {
    pub rel: String,
    pub kind: DiskEntryKind,
}

/// Result of a recursive folder listing.
#[derive(Debug, Default)]
pub(crate) struct DiskListing {
    pub entries: Vec<DiskEntry>,
    /// Backups left by an interrupted download commit (see `storage::download`).
    pub download_backups: Vec<PathBuf>,
}

/// Recursively lists the entries under `base`, skipping Syncthing-internal files.
#[allow(dead_code)]
pub(crate) fn collect_files_recursive(
    base: &Path,
    prefix: &str,
    out: &mut DiskListing,
) -> io::Result<()> {
    collect_files_recursive_with_symlinks(base, prefix, false, out)
}

/// Recursively lists the entries under `base`, skipping Syncthing-internal files.
/// When `sync_symlinks` is `true`, symbolic links are recorded as [`DiskEntryKind::Symlink`]
/// without traversing into symlinked directories.
pub(crate) fn collect_files_recursive_with_symlinks(
    base: &Path,
    prefix: &str,
    sync_symlinks: bool,
    out: &mut DiskListing,
) -> io::Result<()> {
    for entry_res in fs::read_dir(base)? {
        let entry = entry_res?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Internal files only: download temp files, local databases and Syncthing markers.
        // Pulls ignore the same names (`names::is_syncable_name`).
        if super::names::is_internal_name(&name) {
            if super::download::is_download_backup_name(&name) {
                out.download_backups.push(entry.path());
            }
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{}/{}", prefix, name)
        };
        let path = entry.path();
        if sync_symlinks {
            if let Ok(sym_meta) = fs::symlink_metadata(&path) {
                if sym_meta.file_type().is_symlink() {
                    if let Ok(target) = read_link_target(&path) {
                        out.entries.push(DiskEntry {
                            rel,
                            kind: DiskEntryKind::Symlink(target),
                        });
                    } else {
                        out.entries.push(DiskEntry {
                            rel,
                            kind: DiskEntryKind::Unreadable,
                        });
                    }
                    continue;
                }
            }
        }
        // The directory entry type needs no extra `stat()`; fall back to one for links and
        // for platforms that do not report it.
        let file_type = match entry.file_type() {
            Ok(ft) if ft.is_dir() || ft.is_file() => Ok(ft),
            _ => fs::metadata(&path).map(|m| m.file_type()),
        };
        match file_type {
            Ok(ft) if ft.is_dir() => {
                out.entries.push(DiskEntry {
                    rel: rel.clone(),
                    kind: DiskEntryKind::Dir,
                });
                collect_files_recursive_with_symlinks(&path, &rel, sync_symlinks, out)?;
            }
            Ok(ft) if ft.is_file() => out.entries.push(DiskEntry {
                rel,
                kind: DiskEntryKind::File,
            }),
            Ok(_) => {}
            Err(_) => out.entries.push(DiskEntry {
                rel,
                kind: DiskEntryKind::Unreadable,
            }),
        }
    }
    Ok(())
}

#[cfg(not(target_env = "newlib"))]
fn read_link_target(path: &Path) -> io::Result<String> {
    fs::read_link(path).map(|t| t.to_string_lossy().into_owned())
}

#[cfg(target_env = "newlib")]
fn read_link_target(_path: &Path) -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symbolic links are not supported on this platform",
    ))
}
