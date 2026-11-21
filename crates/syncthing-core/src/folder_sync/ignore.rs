//! Pure-Rust `.stignore` parser and matcher, compatible with Syncthing's ignore pattern syntax.
//!
//! Supported features:
//! - Blank lines and `// ...` comments (ignored)
//! - `#include <relpath>` (relative to `folder_root`, confined inside `folder_root`, max depth 8, cycle-guarded)
//! - `(?i)` prefix for case-insensitive matching
//! - `(?d)` prefix marking a pattern as deletable (parsed and tracked via [`IgnoreMatcher::is_deletable`])
//! - `!` prefix for negation (un-ignores a matching path)
//! - Root-anchored patterns (`/foo` matches at the folder root; `foo` matches any component or suffix)
//! - Glob wildcards: `*` (any non-`/` chars), `**` (any chars across `/`), `?` (one non-`/` char), `[abc]` / `[a-z]` / `[!abc]` character classes
//! - Directory-suffix trailing `/` (matches a directory and everything inside it)
//! - `.stignore` itself is always ignored (matching Syncthing)

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Maximum `#include` nesting depth (guards against deeply nested or cyclic includes).
const MAX_INCLUDE_DEPTH: usize = 8;

/// One compiled rule from `.stignore` (or an `#include`d file).
#[derive(Debug, Clone)]
struct IgnoreRule {
    negated: bool,
    deletable: bool,
    case_insensitive: bool,
    anchored: bool,
    dir_only: bool,
    pattern: String,
}

/// Pure-Rust `.stignore` matcher for a Syncthing shared folder.
#[derive(Debug, Clone, Default)]
pub struct IgnoreMatcher {
    rules: Vec<IgnoreRule>,
}

impl IgnoreMatcher {
    /// Empty matcher that ignores only `.stignore` itself.
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }

    /// Loads `<folder_root>/.stignore` and any `#include`d files inside `folder_root`.
    /// Missing `.stignore` (or unreadable `#include` targets) leaves those rules empty rather
    /// than failing the scan.
    pub fn load(folder_root: &Path) -> Self {
        let stignore_path = folder_root.join(".stignore");
        let Ok(content) = fs::read_to_string(&stignore_path) else {
            return Self::empty();
        };
        let canonical_root =
            fs::canonicalize(folder_root).unwrap_or_else(|_| normalize_lexical(folder_root));
        let mut visited = HashSet::new();
        if let Ok(canon) = fs::canonicalize(&stignore_path) {
            visited.insert(canon);
        } else {
            visited.insert(normalize_lexical(&stignore_path));
        }
        let mut rules = Vec::new();
        parse_lines(&content, &canonical_root, 0, &mut visited, &mut rules);
        Self { rules }
    }

    /// Parses `.stignore` lines directly from text (without `#include` filesystem access).
    pub fn parse(content: &str) -> Self {
        let mut rules = Vec::new();
        let mut visited = HashSet::new();
        parse_lines(
            content,
            Path::new(""),
            MAX_INCLUDE_DEPTH,
            &mut visited,
            &mut rules,
        );
        Self { rules }
    }

    /// Returns `true` if `rel_path` (slash-separated, relative to `folder_root`) should be ignored.
    ///
    /// `.stignore` itself is always ignored. For nested paths, if any parent directory matches a
    /// non-negated ignore rule (and no later negation overrides it), the descendant is ignored too.
    pub fn is_ignored(&self, rel_path: &str) -> bool {
        self.evaluate(rel_path).0
    }

    /// Returns `true` if `rel_path` is ignored by a rule carrying the `(?d)` deletable prefix
    /// (meaning Syncthing may delete it if it blocks removing a parent directory).
    pub fn is_deletable(&self, rel_path: &str) -> bool {
        let (ignored, deletable) = self.evaluate(rel_path);
        ignored && deletable
    }

    fn evaluate(&self, rel_path: &str) -> (bool, bool) {
        let norm = rel_path.trim_matches('/');
        if norm.is_empty() {
            return (false, false);
        }
        if norm == ".stignore"
            || norm == ".stversions"
            || norm
                .split('/')
                .any(|c| c == ".stignore" || c == ".stversions")
        {
            return (true, false);
        }
        if self.rules.is_empty() {
            return (false, false);
        }

        // Syncthing evaluates rules top-to-bottom; the first matching rule wins (including `!`).
        // Also check parent directories so ignoring `dir` / `dir/` ignores `dir/file` unless a
        // negation rule explicitly un-ignores a path first.
        for rule in &self.rules {
            if rule_matches(rule, norm) {
                return (!rule.negated, rule.deletable);
            }
        }
        // Check ancestor directory prefixes when no rule matched the full path directly.
        let mut pos = 0;
        while let Some(slash) = norm[pos..].find('/') {
            let prefix = &norm[..pos + slash];
            for rule in &self.rules {
                if rule_matches_dir(rule, prefix) {
                    return (!rule.negated, rule.deletable);
                }
            }
            pos += slash + 1;
        }

        (false, false)
    }
}

fn parse_lines(
    content: &str,
    canonical_root: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<IgnoreRule>,
) {
    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#include") {
            if depth < MAX_INCLUDE_DEPTH && (rest.starts_with(' ') || rest.starts_with('\t')) {
                let rel_inc = rest.trim();
                if let Some(inc_path) = resolve_confined_include(canonical_root, rel_inc) {
                    let key = fs::canonicalize(&inc_path).unwrap_or_else(|_| inc_path.clone());
                    if visited.insert(key) {
                        if let Ok(inc_content) = fs::read_to_string(&inc_path) {
                            parse_lines(&inc_content, canonical_root, depth + 1, visited, out);
                        }
                    }
                }
            }
            continue;
        }
        if let Some(rule) = parse_rule(trimmed) {
            out.push(rule);
        }
    }
}

fn resolve_confined_include(canonical_root: &Path, rel_inc: &str) -> Option<PathBuf> {
    if rel_inc.is_empty() || canonical_root.as_os_str().is_empty() {
        return None;
    }
    let rel_path = Path::new(rel_inc);
    if rel_path.is_absolute() {
        return None;
    }
    let mut joined = canonical_root.to_path_buf();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(seg) => joined.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    let canonical_target = fs::canonicalize(&joined).ok()?;
    if !canonical_target.starts_with(canonical_root) {
        return None;
    }
    Some(canonical_target)
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn parse_rule(raw: &str) -> Option<IgnoreRule> {
    let mut s = raw;
    let mut negated = false;
    let mut deletable = false;
    let mut case_insensitive = false;

    // Prefixes `!`, `(?d)`, `(?i)` may appear in any order at the start of a rule.
    loop {
        if let Some(rest) = s.strip_prefix('!') {
            negated = true;
            s = rest;
        } else if let Some(rest) = s.strip_prefix("(?d)") {
            deletable = true;
            s = rest;
        } else if let Some(rest) = s.strip_prefix("(?i)") {
            case_insensitive = true;
            s = rest;
        } else {
            break;
        }
    }

    if s.is_empty() {
        return None;
    }

    let anchored = s.starts_with('/');
    if anchored {
        s = &s[1..];
    }
    let dir_only = s.ends_with('/') && s.len() > 1;
    if dir_only {
        s = s.trim_end_matches('/');
    }
    if s.is_empty() {
        return None;
    }

    let pattern = if case_insensitive {
        s.to_ascii_lowercase()
    } else {
        s.to_owned()
    };

    Some(IgnoreRule {
        negated,
        deletable,
        case_insensitive,
        anchored,
        dir_only,
        pattern,
    })
}

fn rule_matches_dir(rule: &IgnoreRule, dir_rel: &str) -> bool {
    rule_matches_inner(rule, dir_rel)
}

fn rule_matches(rule: &IgnoreRule, rel_path: &str) -> bool {
    if rule.dir_only {
        // A trailing-slash rule `foo/` matches anything inside `foo/`, or `foo` itself if queried.
        let mut pos = 0;
        while let Some(slash) = rel_path[pos..].find('/') {
            let prefix = &rel_path[..pos + slash];
            if rule_matches_inner(rule, prefix) {
                return true;
            }
            pos += slash + 1;
        }
    }
    rule_matches_inner(rule, rel_path)
}

fn rule_matches_inner(rule: &IgnoreRule, rel_path: &str) -> bool {
    let candidate_owned;
    let candidate = if rule.case_insensitive {
        candidate_owned = rel_path.to_ascii_lowercase();
        candidate_owned.as_str()
    } else {
        rel_path
    };

    let pat = rule.pattern.as_str();
    if rule.anchored || pat.contains('/') {
        // Anchored or multi-segment patterns match from the folder root (or if unanchored without
        // leading `/`, Syncthing matches either from root or any subpath when `/` is inside?
        // Actually in Syncthing: `/a/b` and `a/b` both match relative to root, and `**/a/b` matches
        // at any depth).
        glob_match(pat, candidate)
    } else {
        // Unanchored single-segment pattern (e.g. `*.tmp`, `build`): matches the basename or any
        // path component.
        candidate.split('/').any(|comp| glob_match(pat, comp))
    }
}

/// Glob matcher supporting `*` (non-`/`), `**` (any including `/`), `?` (one non-`/`), and `[...]`.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_chars(&p, &t)
}

fn glob_match_chars(p: &[char], t: &[char]) -> bool {
    if p.is_empty() {
        return t.is_empty();
    }
    // Check for `**`
    if p.len() >= 2 && p[0] == '*' && p[1] == '*' {
        // Collapse consecutive `*`s beyond `**`
        let mut next_p = 2;
        while next_p < p.len() && p[next_p] == '*' {
            next_p += 1;
        }
        // Special case: `**/` can match zero or more leading directories (including empty).
        if next_p < p.len() && p[next_p] == '/' && glob_match_chars(&p[next_p + 1..], t) {
            return true;
        }
        for i in 0..=t.len() {
            if glob_match_chars(&p[next_p..], &t[i..]) {
                return true;
            }
        }
        return false;
    }

    match p[0] {
        '*' => {
            for i in 0..=t.len() {
                if i > 0 && t[i - 1] == '/' {
                    break;
                }
                if glob_match_chars(&p[1..], &t[i..]) {
                    return true;
                }
            }
            false
        }
        '?' => !t.is_empty() && t[0] != '/' && glob_match_chars(&p[1..], &t[1..]),
        '[' => {
            if t.is_empty() || t[0] == '/' {
                return false;
            }
            if let Some((matched, next_p)) = match_char_class(&p[1..], t[0]) {
                matched && glob_match_chars(&p[1 + next_p..], &t[1..])
            } else {
                // Malformed `[`: treat literally.
                t[0] == '[' && glob_match_chars(&p[1..], &t[1..])
            }
        }
        ch => !t.is_empty() && t[0] == ch && glob_match_chars(&p[1..], &t[1..]),
    }
}

/// Parses `[...]` starting just after `[`. Returns `(matched, chars_consumed_up_to_closing_bracket)`.
fn match_char_class(p: &[char], ch: char) -> Option<(bool, usize)> {
    if p.is_empty() {
        return None;
    }
    let mut idx = 0;
    let negated = if p[0] == '!' || p[0] == '^' {
        idx = 1;
        true
    } else {
        false
    };
    let mut matched = false;
    let mut found_close = false;
    let start = idx;
    while idx < p.len() {
        if p[idx] == ']' && idx > start {
            found_close = true;
            idx += 1;
            break;
        }
        if idx + 2 < p.len() && p[idx + 1] == '-' && p[idx + 2] != ']' {
            let lo = p[idx];
            let hi = p[idx + 2];
            if lo <= ch && ch <= hi {
                matched = true;
            }
            idx += 3;
        } else {
            if p[idx] == ch {
                matched = true;
            }
            idx += 1;
        }
    }
    if !found_close {
        return None;
    }
    Some((if negated { !matched } else { matched }, idx))
}
