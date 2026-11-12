//! Escaping of text fields stored in the `|`-separated, line-based `.cfg` files.
//!
//! Device names, folder IDs and folder labels come from remote peers (`Hello`, `ClusterConfig`).
//! Written verbatim, a `|` or a line break in one of them would forge extra fields or whole
//! configuration lines (e.g. a share on an arbitrary `ux0:` path), so every text field is
//! percent-escaped on save and unescaped on load.

/// Characters with a meaning in the `.cfg` format, and `%` itself.
const ESCAPES: &[(char, &str)] = &[('%', "%25"), ('|', "%7C"), ('\n', "%0A"), ('\r', "%0D")];

/// Escapes `field` so it can be stored as one `|`-separated field on a single line.
pub fn escape_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    for ch in field.chars() {
        match ESCAPES.iter().find(|(c, _)| *c == ch) {
            Some((_, escaped)) => out.push_str(escaped),
            None => out.push(ch),
        }
    }
    out
}

/// Reverses [`escape_field`]. Any other `%` sequence is kept as is, so files written before
/// escaping existed still load unchanged.
pub fn unescape_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut rest = field;
    while let Some(pos) = rest.find('%') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let decoded = ESCAPES.iter().find(|(_, escaped)| {
            rest.get(..escaped.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(escaped))
        });
        match decoded {
            Some((ch, escaped)) => {
                out.push(*ch);
                rest = &rest[escaped.len()..];
            }
            None => {
                out.push('%');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
