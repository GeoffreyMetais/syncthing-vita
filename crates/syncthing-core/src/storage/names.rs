//! Entry names inside a shared folder: which ones are Syncthing-internal, and which names
//! announced by a peer designate an entry strictly inside the folder.
//!
//! Scans and pulls must agree on these rules: an entry that is pulled but never listed by a
//! scan would be announced as deleted (and then deleted on every peer).

/// Marker files and directories Syncthing keeps at a folder root and never synchronizes.
const SYNCTHING_MARKERS: [&str; 3] = [".stfolder", ".stignore", ".stversions"];

/// Returns `true` if `name` (one path component) is a Syncthing-internal file: download temp
/// files and backups, local databases, folder markers. Internal files are neither indexed nor
/// pulled. Any other dot-file (`.stylelintrc`, `.steam`, ...) is an ordinary entry.
pub fn is_internal_name(name: &str) -> bool {
    name.starts_with(".syncthing")
        || name.starts_with("~syncthing~")
        || SYNCTHING_MARKERS.contains(&name)
}

/// Returns `true` if `name`, as announced by a peer, is a relative path of plain components
/// that can be synchronized. Rejected: empty names, absolute paths, empty / `.` / `..`
/// components, backslashes (a separator on some platforms), control characters and
/// Syncthing-internal components.
pub fn is_syncable_name(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().any(|c| c == '\\' || c.is_control())
        && name
            .split('/')
            .all(|comp| !comp.is_empty() && comp != "." && comp != ".." && !is_internal_name(comp))
}
