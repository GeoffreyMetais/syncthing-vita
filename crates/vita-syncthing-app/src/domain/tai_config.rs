//! Pure editor of taiHEN's `config.txt` (`ux0:tai/config.txt` or `ur0:tai/config.txt`).
//!
//! The file lists plugins per section: a `*<TITLE ID>` / `*main` / `*KERNEL` header line, then
//! one module path per line; `#` starts a comment. taiHEN loads the plugins of `*main` into
//! `SceShell` (the LiveArea). Only lines naming one of our module paths are ever added or
//! removed: comments, unrelated sections and entries, blank lines and the line endings of the
//! file (CRLF or LF) are kept as they are. Choosing the active file and every file access
//! belong to the application / infrastructure layers.

/// Header of the section whose plugins taiHEN loads into `SceShell`.
pub const MAIN_SECTION: &str = "*main";

/// Line ending used by `text`: CRLF as soon as one line uses it, LF otherwise.
pub fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// True for a section header line (`*main`, `*KERNEL`, `*NPXS10015`...).
fn is_section_header(line: &str) -> bool {
    line.trim_start().starts_with('*')
}

/// True for the `*main` section header.
fn is_main_header(line: &str) -> bool {
    line.trim() == MAIN_SECTION
}

/// True if the entry `line` names `path` (PS Vita paths are case-insensitive).
fn names_path(line: &str, path: &str) -> bool {
    !is_section_header(line) && line.trim().eq_ignore_ascii_case(path.trim())
}

/// Lines of `text` without their line endings (no trailing empty line for a final newline).
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

/// Joins `lines` with `eol`, ending with `eol` when `trailing` is set.
fn join_lines(lines: &[&str], eol: &str, trailing: bool) -> String {
    let mut out = lines.join(eol);
    if trailing && !lines.is_empty() {
        out.push_str(eol);
    }
    out
}

/// True if a `*main` section of `text` lists `path`.
pub fn has_main_entry(text: &str, path: &str) -> bool {
    let mut in_main = false;
    for line in split_lines(text) {
        if is_section_header(line) {
            in_main = is_main_header(line);
        } else if in_main && names_path(line, path) {
            return true;
        }
    }
    false
}

/// Returns `text` with `path` listed exactly once, at the end of the first `*main` section
/// (created at the end of the file when there is none). Every other line naming `path` or
/// one of `stale` (e.g. the same module in the other `tai` directory) is removed, whatever
/// its section. Applying it again to its own result changes nothing.
pub fn add_main_entry(text: &str, path: &str, stale: &[&str]) -> String {
    let eol = line_ending(text);
    let mut lines: Vec<&str> = split_lines(text)
        .into_iter()
        .filter(|l| !names_path(l, path) && !stale.iter().any(|s| names_path(l, s)))
        .collect();
    match lines.iter().position(|l| is_main_header(l)) {
        Some(header) => {
            let mut end = lines[header + 1..]
                .iter()
                .position(|l| is_section_header(l))
                .map_or(lines.len(), |i| header + 1 + i);
            // After the section's last entry, before the blank lines separating it.
            while end > header + 1 && lines[end - 1].trim().is_empty() {
                end -= 1;
            }
            lines.insert(end, path);
        }
        None => {
            lines.push(MAIN_SECTION);
            lines.push(path);
        }
    }
    join_lines(&lines, eol, true)
}

/// Returns `text` without the lines naming one of `paths` (in any section). Section headers,
/// even when left empty, and every other line are kept.
pub fn remove_entries(text: &str, paths: &[&str]) -> String {
    let lines: Vec<&str> = split_lines(text)
        .into_iter()
        .filter(|l| !paths.iter().any(|p| names_path(l, p)))
        .collect();
    join_lines(&lines, line_ending(text), text.ends_with('\n'))
}
