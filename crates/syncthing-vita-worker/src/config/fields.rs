//! Escaping of the text fields of the app's `|`-separated `.cfg` files.
//!
//! The app writes every text field (device names, addresses, folder IDs, labels, paths) with
//! `config_fields::escape_field` (`%` -> `%25`, `|` -> `%7C`, line breaks -> `%0A` / `%0D`):
//! names and folder IDs come from remote peers and must never forge fields or lines. This is
//! the same encoding and decoding, checked against the app's by `tests/config_formats.rs`.

/// Characters with a meaning in the `.cfg` format, and `%` itself.
const ESCAPES: &[(char, &str)] = &[('%', "%25"), ('|', "%7C"), ('\n', "%0A"), ('\r', "%0D")];

/// Encodes `field` as the app's `config_fields::escape_field` does (`pending_offers.cfg` is
/// written by the worker and read by the app).
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

/// Decodes a field written by the app. Any other `%` sequence is kept as is (files written
/// before escaping existed load unchanged).
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
