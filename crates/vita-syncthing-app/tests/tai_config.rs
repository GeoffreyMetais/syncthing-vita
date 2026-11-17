//! taiHEN `config.txt` editor (`domain::tai_config`): `*main` entry lookup, insertion into an
//! existing section or a new one, removal, deduplication of stale entries, idempotency, and
//! preservation of comments, unrelated sections and line endings.

use vita_syncthing_app::domain::tai_config::{
    add_main_entry, has_main_entry, line_ending, remove_entries, MAIN_SECTION,
};

const LAUNCHER: &str = "ur0:tai/syncthing_launcher.suprx";
const UX0_LAUNCHER: &str = "ux0:tai/syncthing_launcher.suprx";
const WORKER: &str = "ur0:tai/syncthing_worker.suprx";

/// A typical HENkaku Ensō `ur0:tai/config.txt`.
const STOCK: &str = "# This file is used as an alternative if ux0:tai/config.txt is not found.\n\
*KERNEL\n\
# henkaku.skprx is hard-coded to load and is not listed here\n\
ur0:tai/nonpdrm.skprx\n\
*main\n\
# main is a special titleid for SceShell\n\
ur0:tai/henkaku.suprx\n\
\n\
*NPXS10015\n\
# this is for modifying the version string\n\
ur0:tai/henkaku.suprx\n";

#[test]
fn test_main_section_header_and_line_endings() {
    assert_eq!(MAIN_SECTION, "*main");
    assert_eq!(line_ending("a\nb\n"), "\n");
    assert_eq!(line_ending("a\r\nb\r\n"), "\r\n");
    assert_eq!(line_ending("a\nb\r\n"), "\r\n");
    assert_eq!(line_ending(""), "\n");
}

#[test]
fn test_entry_is_inserted_at_the_end_of_the_main_section() {
    let out = add_main_entry(STOCK, LAUNCHER, &[]);
    let expected = STOCK.replace(
        "ur0:tai/henkaku.suprx\n\n*NPXS10015",
        &format!("ur0:tai/henkaku.suprx\n{LAUNCHER}\n\n*NPXS10015"),
    );
    assert_eq!(out, expected);
    assert!(has_main_entry(&out, LAUNCHER));
    assert!(!has_main_entry(STOCK, LAUNCHER));
}

#[test]
fn test_adding_twice_changes_nothing() {
    let once = add_main_entry(STOCK, LAUNCHER, &[WORKER]);
    let twice = add_main_entry(&once, LAUNCHER, &[WORKER]);
    assert_eq!(once, twice);
    assert_eq!(once.matches(LAUNCHER).count(), 1);
}

#[test]
fn test_main_section_is_created_when_missing() {
    let text = "*KERNEL\nur0:tai/nonpdrm.skprx\n";
    assert_eq!(
        add_main_entry(text, LAUNCHER, &[]),
        format!("*KERNEL\nur0:tai/nonpdrm.skprx\n*main\n{LAUNCHER}\n")
    );
    // An empty file (or one without a final newline) gets a well-formed section.
    assert_eq!(
        add_main_entry("", LAUNCHER, &[]),
        format!("*main\n{LAUNCHER}\n")
    );
    assert_eq!(
        add_main_entry("*KERNEL", LAUNCHER, &[]),
        format!("*KERNEL\n*main\n{LAUNCHER}\n")
    );
}

#[test]
fn test_main_section_as_last_section_without_final_newline() {
    let text = "*KERNEL\nur0:tai/a.skprx\n*main\nur0:tai/henkaku.suprx";
    assert_eq!(
        add_main_entry(text, LAUNCHER, &[]),
        format!("*KERNEL\nur0:tai/a.skprx\n*main\nur0:tai/henkaku.suprx\n{LAUNCHER}\n")
    );
}

#[test]
fn test_empty_main_section_followed_by_blank_lines() {
    let text = "*main\n\n\n*KERNEL\nur0:tai/a.skprx\n";
    assert_eq!(
        add_main_entry(text, LAUNCHER, &[]),
        format!("*main\n{LAUNCHER}\n\n\n*KERNEL\nur0:tai/a.skprx\n")
    );
}

#[test]
fn test_only_the_first_main_section_receives_the_entry() {
    let text = "*main\nur0:tai/a.suprx\n*main\nur0:tai/b.suprx\n";
    assert_eq!(
        add_main_entry(text, LAUNCHER, &[]),
        format!("*main\nur0:tai/a.suprx\n{LAUNCHER}\n*main\nur0:tai/b.suprx\n")
    );
}

#[test]
fn test_crlf_files_keep_crlf() {
    let crlf = STOCK.replace('\n', "\r\n");
    let out = add_main_entry(&crlf, LAUNCHER, &[]);
    assert!(out.contains(&format!(
        "ur0:tai/henkaku.suprx\r\n{LAUNCHER}\r\n\r\n*NPXS10015"
    )));
    assert!(
        !out.replace("\r\n", "").contains('\n'),
        "LF-only line in {out:?}"
    );
    assert!(has_main_entry(&out, LAUNCHER));
    let back = remove_entries(&out, &[LAUNCHER]);
    assert_eq!(back, crlf);
}

#[test]
fn test_duplicates_and_stale_entries_are_dropped_in_every_section() {
    let text = format!(
        "*KERNEL\n{WORKER}\n*main\n{LAUNCHER}\nur0:tai/henkaku.suprx\n  {}  \n{UX0_LAUNCHER}\n*ALL\n{UX0_LAUNCHER}\n",
        LAUNCHER.to_uppercase()
    );
    let out = add_main_entry(&text, LAUNCHER, &[UX0_LAUNCHER, WORKER]);
    assert_eq!(
        out,
        format!("*KERNEL\n*main\nur0:tai/henkaku.suprx\n{LAUNCHER}\n*ALL\n")
    );
}

#[test]
fn test_comments_and_section_headers_are_never_removed() {
    let text = format!("*main\n# {LAUNCHER}\n{LAUNCHER}\n*{LAUNCHER}\n");
    let out = remove_entries(&text, &[LAUNCHER, &format!("*{LAUNCHER}")]);
    assert_eq!(out, format!("*main\n# {LAUNCHER}\n*{LAUNCHER}\n"));
}

#[test]
fn test_entry_lookup_is_limited_to_main_sections() {
    let text = format!("*KERNEL\n{LAUNCHER}\n*main\nur0:tai/henkaku.suprx\n");
    assert!(!has_main_entry(&text, LAUNCHER));
    let text = format!("*main\n {} \n", LAUNCHER.to_uppercase());
    assert!(has_main_entry(&text, LAUNCHER));
    // A section after `*main` ends it.
    let text = format!("*main\nur0:tai/a.suprx\n*NPXS10015\n{LAUNCHER}\n");
    assert!(!has_main_entry(&text, LAUNCHER));
    // A `*main`-like header is not `*main`.
    let text = format!("*mainx\n{LAUNCHER}\n");
    assert!(!has_main_entry(&text, LAUNCHER));
    assert!(!has_main_entry("", LAUNCHER));
}

#[test]
fn test_remove_keeps_everything_else_and_the_final_newline_state() {
    let installed = add_main_entry(STOCK, LAUNCHER, &[]);
    assert_eq!(remove_entries(&installed, &[LAUNCHER, WORKER]), STOCK);
    // Nothing of ours: unchanged.
    assert_eq!(remove_entries(STOCK, &[LAUNCHER]), STOCK);
    let no_final_newline = format!("*main\nur0:tai/a.suprx\n{LAUNCHER}");
    assert_eq!(
        remove_entries(&no_final_newline, &[LAUNCHER]),
        "*main\nur0:tai/a.suprx"
    );
    // The section header stays even when emptied.
    assert_eq!(
        remove_entries(&format!("*main\n{LAUNCHER}\n"), &[LAUNCHER]),
        "*main\n"
    );
    assert_eq!(remove_entries("", &[LAUNCHER]), "");
    assert_eq!(
        remove_entries(&format!("{LAUNCHER}\n"), &[LAUNCHER]),
        "",
        "a file holding only our entry becomes empty"
    );
}
