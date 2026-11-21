//! `.stversions` file versioning for overwritten and remotely deleted files.
//!
//! Supported policies (configured per folder via [`FsSyncStorage::set_folder_versioning`]):
//! - [`FileVersioning::None`] (default): files are overwritten or removed directly.
//! - [`FileVersioning::TrashCan`]: moves the previous file into `<folder_root>/.stversions/<rel_path>`,
//!   optionally pruning entries older than `cleanout_days`.
//! - [`FileVersioning::Simple`]: archives the previous file as
//!   `<folder_root>/.stversions/<dir>/<stem>~<YYYYMMDD-HHMMSS>[.<ext>]` and prunes the oldest
//!   versions beyond `keep`.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::names::is_internal_name;
use super::vita_fs::ensure_dir_exists;

/// Directory name at each folder root holding archived file versions.
pub const STVERSIONS_DIR: &str = ".stversions";

/// Per-folder file versioning policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileVersioning {
    /// No versioning: overwritten or deleted files are not archived.
    #[default]
    None,
    /// Moves overwritten or deleted files to `<folder_root>/.stversions/<rel_path>`.
    /// `cleanout_days == 0` disables automatic age-based cleanup.
    TrashCan { cleanout_days: u32 },
    /// Archives overwritten or deleted files with a `~YYYYMMDD-HHMMSS` timestamp suffix and
    /// retains at most `keep` versions per file (`keep == 0` keeps all versions).
    Simple { keep: usize },
}

/// Archives `file_path` (which must currently reside under `folder_root` at `rel_path`) according
/// to `policy` before it is overwritten or deleted.
///
/// Does nothing when `policy` is [`FileVersioning::None`] or when `file_path` is not a regular
/// file (or is itself inside `.stversions`).
pub fn archive_file(
    folder_root: &Path,
    rel_path: &str,
    file_path: &Path,
    policy: FileVersioning,
) -> io::Result<()> {
    archive_file_at(folder_root, rel_path, file_path, policy, SystemTime::now())
}

/// Same as [`archive_file`] with an explicit timestamp (useful for deterministic tests).
pub fn archive_file_at(
    folder_root: &Path,
    rel_path: &str,
    file_path: &Path,
    policy: FileVersioning,
    now: SystemTime,
) -> io::Result<()> {
    if policy == FileVersioning::None {
        return Ok(());
    }
    let Ok(meta) = fs::symlink_metadata(file_path) else {
        return Ok(());
    };
    if meta.file_type().is_symlink() || !meta.file_type().is_file() {
        return Ok(());
    }
    let clean_rel = sanitize_rel_path(rel_path);
    if clean_rel.as_os_str().is_empty() {
        return Ok(());
    }

    let versions_root = folder_root.join(STVERSIONS_DIR);
    ensure_dir_exists(&versions_root)?;

    match policy {
        FileVersioning::None => Ok(()),
        FileVersioning::TrashCan { cleanout_days } => {
            let dest = versions_root.join(&clean_rel);
            if let Some(parent) = dest.parent() {
                ensure_dir_exists(parent)?;
            }
            move_or_copy_replace(file_path, &dest)?;
            if cleanout_days > 0 {
                let cutoff = now
                    .checked_sub(Duration::from_secs(u64::from(cleanout_days) * 86_400))
                    .unwrap_or(UNIX_EPOCH);
                let _ = prune_older_than(&versions_root, cutoff);
            }
            Ok(())
        }
        FileVersioning::Simple { keep } => {
            let stamped_rel = timestamped_rel_path(&clean_rel, now);
            let mut dest = versions_root.join(&stamped_rel);
            // Guarantee uniqueness if two archives occur within the same second.
            if dest.exists() {
                dest = unique_collision_path(&dest);
            }
            if let Some(parent) = dest.parent() {
                ensure_dir_exists(parent)?;
            }
            move_or_copy_replace(file_path, &dest)?;
            if keep > 0 {
                prune_simple_versions(&versions_root, &clean_rel, keep)?;
            }
            Ok(())
        }
    }
}

/// Formats `<stem>~<YYYYMMDD-HHMMSS>[.<ext>]` under the same relative parent directory.
pub fn format_versioned_filename(file_name: &str, timestamp: SystemTime) -> String {
    let stamp = format_utc_timestamp(timestamp);
    match split_stem_ext(file_name) {
        (stem, Some(ext)) => format!("{stem}~{stamp}.{ext}"),
        (stem, None) => format!("{stem}~{stamp}"),
    }
}

/// Removes archived versions in `versions_root` whose modification time is older than `cutoff`.
pub fn prune_older_than(versions_root: &Path, cutoff: SystemTime) -> io::Result<usize> {
    if !versions_root.is_dir() {
        return Ok(0);
    }
    let mut removed = 0usize;
    prune_older_than_recursive(versions_root, cutoff, &mut removed)?;
    Ok(removed)
}

fn prune_older_than_recursive(
    dir: &Path,
    cutoff: SystemTime,
    removed: &mut usize,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            prune_older_than_recursive(&path, cutoff, removed)?;
            // Clean up now-empty directories inside `.stversions`.
            if fs::read_dir(&path)
                .map(|mut it| it.next().is_none())
                .unwrap_or(false)
            {
                let _ = fs::remove_dir(&path);
            }
        } else if meta.is_file() {
            if let Ok(mtime) = meta.modified() {
                if mtime < cutoff && fs::remove_file(&path).is_ok() {
                    *removed += 1;
                }
            }
        }
    }
    Ok(())
}

fn prune_simple_versions(versions_root: &Path, clean_rel: &Path, keep: usize) -> io::Result<()> {
    let target_dir = match clean_rel.parent() {
        Some(p) if !p.as_os_str().is_empty() => versions_root.join(p),
        _ => versions_root.to_path_buf(),
    };
    let Some(orig_name) = clean_rel.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    let (stem, ext) = split_stem_ext(orig_name);
    let prefix = format!("{stem}~");
    let suffix = ext.map(|e| format!(".{e}"));

    let mut matches: Vec<(String, PathBuf)> = Vec::new();
    let Ok(entries) = fs::read_dir(&target_dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if matches_versioned_name(name, &prefix, suffix.as_deref()) {
            matches.push((name.to_owned(), path));
        }
    }
    if matches.len() <= keep {
        return Ok(());
    }
    // Filenames contain `~YYYYMMDD-HHMMSS`, so lexicographic ascending order puts oldest first.
    matches.sort_by(|a, b| a.0.cmp(&b.0));
    let to_remove = matches.len() - keep;
    for (_, path) in matches.into_iter().take(to_remove) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn matches_versioned_name(name: &str, prefix: &str, suffix: Option<&str>) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    let stamp_part = match suffix {
        Some(sfx) => match rest.strip_suffix(sfx) {
            Some(s) => s,
            None => return false,
        },
        None => {
            if rest.contains('.') {
                return false;
            }
            rest
        }
    };
    // Match `YYYYMMDD-HHMMSS` (15 chars) or `YYYYMMDD-HHMMSS-<n>` collision suffix.
    let core = stamp_part.get(..15).unwrap_or("");
    let bytes = core.as_bytes();
    bytes.len() == 15
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'-'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
}

fn timestamped_rel_path(clean_rel: &Path, now: SystemTime) -> PathBuf {
    let file_name = clean_rel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let versioned_name = format_versioned_filename(&file_name, now);
    match clean_rel.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(versioned_name),
        _ => PathBuf::from(versioned_name),
    }
}

fn unique_collision_path(base: &Path) -> PathBuf {
    let name = base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = split_stem_ext(&name);
    for idx in 1..=9999u32 {
        let candidate_name = match ext {
            Some(e) => format!("{stem}-{idx}.{e}"),
            None => format!("{stem}-{idx}"),
        };
        let candidate = base.with_file_name(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    base.to_path_buf()
}

fn split_stem_ext(file_name: &str) -> (&str, Option<&str>) {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, Some(ext)),
        _ => (file_name, None),
    }
}

fn sanitize_rel_path(rel_path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for seg in rel_path.split(['/', '\\']) {
        if seg.is_empty() || seg == "." || seg == ".." || is_internal_name(seg) {
            continue;
        }
        let p = Path::new(seg);
        if matches!(p.components().next(), Some(Component::Normal(_))) {
            out.push(seg);
        }
    }
    out
}

fn move_or_copy_replace(src: &Path, dest: &Path) -> io::Result<()> {
    let _ = fs::remove_file(dest);
    if fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    fs::copy(src, dest)?;
    fs::remove_file(src)
}

/// Formats `SystemTime` as UTC `YYYYMMDD-HHMMSS` using Howard Hinnant's civil-from-days algorithm.
fn format_utc_timestamp(ts: SystemTime) -> String {
    let secs = ts
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let sod = secs % 86_400;
    let hour = sod / 3_600;
    let min = (sod % 3_600) / 60;
    let sec = sod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}{month:02}{day:02}-{hour:02}{min:02}{sec:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d as u32)
}
