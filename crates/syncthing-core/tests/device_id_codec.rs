//! Device ID text form: Base32 (RFC 4648) with Luhn mod 32 check characters, user-input
//! parsing with its typed errors, short-ID prefix matching, hex persistence and `Debug`.
//!
//! The expected strings below were derived by hand from the algorithm (Base32 of the 32
//! bytes, then one Luhn mod 32 character per 13 Base32 characters, weights 1 and 2 starting
//! at weight 1), so they do not depend on the implementation under test.

use syncthing_core::{DeviceId, DeviceIdParseError};

/// Byte 0 = 0x08: first Base32 character is `B` (value 1); check character `7`.
fn id_a() -> DeviceId {
    let mut raw = [0u8; 32];
    raw[0] = 0x08;
    DeviceId(raw)
}
const ID_A: &str = "BAAAAAA-AAAAAA7-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA";

/// Byte 1 = 0x40: second Base32 character is `B` (weight 2, value 1), check character `6`.
fn id_b() -> DeviceId {
    let mut raw = [0u8; 32];
    raw[1] = 0x40;
    DeviceId(raw)
}
const ID_B: &str = "ABAAAAA-AAAAAA6-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA";

/// Byte 0 = 0x04: second Base32 character is `Q` (value 16, weight 2), check character `7`.
fn id_c() -> DeviceId {
    let mut raw = [0u8; 32];
    raw[0] = 0x04;
    DeviceId(raw)
}
const ID_C: &str = "AQAAAAA-AAAAAA7-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA";

fn without_hyphens(s: &str) -> String {
    s.chars().filter(|&c| c != '-').collect()
}

#[test]
fn luhn_check_characters_match_hand_computed_vectors() {
    assert_eq!(id_a().to_syncthing_string(), ID_A);
    assert_eq!(id_b().to_syncthing_string(), ID_B);
    assert_eq!(id_c().to_syncthing_string(), ID_C);
    assert_eq!(DeviceId::parse_user_id(ID_A).unwrap(), id_a());
    assert_eq!(DeviceId::parse_user_id(ID_B).unwrap(), id_b());
    assert_eq!(DeviceId::parse_user_id(ID_C).unwrap(), id_c());
}

#[test]
fn all_zero_id_is_all_a_with_a_zero_check_character() {
    let zero = DeviceId([0u8; 32]);
    assert_eq!(
        zero.to_syncthing_string(),
        "AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA-AAAAAAA"
    );
    assert_eq!(zero.short(), "AAAAAAA");
}

#[test]
fn display_and_debug_show_the_syncthing_string() {
    assert_eq!(format!("{}", id_a()), ID_A);
    assert_eq!(format!("{:?}", id_a()), format!("DeviceId({ID_A})"));
}

#[test]
fn parse_accepts_hyphens_spaces_underscores_and_lowercase() {
    let mixed = "baaaaaa aaaaaa7_aaaaaaa-aaaaaaa-aaaaaaa-aaaaaaa-aaaaaaa-aaaaaaa";
    assert_eq!(DeviceId::parse_user_id(mixed).unwrap(), id_a());
    assert_eq!(
        DeviceId::parse_user_id(&without_hyphens(ID_B)).unwrap(),
        id_b()
    );
    assert_eq!(
        DeviceId::parse_user_id(&format!("  {ID_C}  ")).unwrap(),
        id_c()
    );
}

#[test]
fn parse_reports_checksum_mismatch_with_the_group_pair() {
    // Check character of group pair 1 (index 13) altered.
    let mut s = without_hyphens(ID_A);
    s.replace_range(13..14, "A");
    assert_eq!(
        DeviceId::parse_user_id(&s),
        Err(DeviceIdParseError::ChecksumMismatch { first_group: 1 })
    );

    // Check character of group pair 2 (index 27) altered.
    let mut s = without_hyphens(ID_A);
    s.replace_range(27..28, "B");
    assert_eq!(
        DeviceId::parse_user_id(&s),
        Err(DeviceIdParseError::ChecksumMismatch { first_group: 3 })
    );

    // Pair 3 (index 41) and pair 4 (index 55).
    let mut s = without_hyphens(ID_A);
    s.replace_range(41..42, "B");
    assert_eq!(
        DeviceId::parse_user_id(&s),
        Err(DeviceIdParseError::ChecksumMismatch { first_group: 5 })
    );
    let mut s = without_hyphens(ID_A);
    s.replace_range(55..56, "B");
    assert_eq!(
        DeviceId::parse_user_id(&s),
        Err(DeviceIdParseError::ChecksumMismatch { first_group: 7 })
    );
}

#[test]
fn parse_checksum_error_in_a_short_prefix_reports_its_pair() {
    // 28 characters: two complete 14-character pairs; the second check character is wrong.
    let mut s = without_hyphens(ID_A)[..28].to_owned();
    assert!(DeviceId::parse_user_id(&s).is_ok());
    s.replace_range(27..28, "B");
    assert_eq!(
        DeviceId::parse_user_id(&s),
        Err(DeviceIdParseError::ChecksumMismatch { first_group: 3 })
    );
}

#[test]
fn parse_accepts_the_raw_52_character_base32_form() {
    // The 52 data characters of ID_A without its four Luhn check characters.
    let raw: String = without_hyphens(ID_A)
        .chars()
        .enumerate()
        .filter(|(i, _)| i % 14 != 13)
        .map(|(_, c)| c)
        .collect();
    assert_eq!(raw.len(), 52);
    assert_eq!(DeviceId::parse_user_id(&raw).unwrap(), id_a());
}

#[test]
fn parse_of_a_two_pair_prefix_is_a_placeholder_matching_the_full_id() {
    let prefix = &without_hyphens(ID_A)[..28];
    let parsed = DeviceId::parse_user_id(prefix).unwrap();
    assert!(parsed.is_short_placeholder());
    assert_eq!(parsed.syncthing_prefix(), prefix);
    assert!(parsed.matches_peer(&id_a()));
}

#[test]
fn parse_length_bounds_are_exact() {
    // Four characters is the minimum prefix; three is rejected.
    let four = DeviceId::parse_user_id("AAAA").unwrap();
    assert!(four.is_short_placeholder());
    assert_eq!(four.syncthing_prefix(), "AAAA");
    assert_eq!(
        DeviceId::parse_user_id("AAA"),
        Err(DeviceIdParseError::TooShort)
    );
    assert_eq!(
        DeviceId::parse_user_id(&"A".repeat(56)),
        Ok(DeviceId([0u8; 32]))
    );
    assert_eq!(
        DeviceId::parse_user_id(&"A".repeat(57)),
        Err(DeviceIdParseError::TooLong(57))
    );
}

#[test]
fn matches_peer_accepts_a_prefix_that_extends_the_other_placeholder() {
    // "AAAAAAAB" extends "AAAAAAA": the two placeholders describe the same device.
    let long = DeviceId::parse_user_id("AAAAAAAB").unwrap();
    let short = DeviceId::parse_user_id("AAAAAAA").unwrap();
    assert!(long.matches_peer(&short));
    assert!(short.matches_peer(&long));
    // Two placeholders with diverging prefixes do not match.
    let other = DeviceId::parse_user_id("AAAAAAAA").unwrap();
    assert!(!long.matches_peer(&other));
    assert!(!other.matches_peer(&long));
}

#[test]
fn matches_peer_prefix_against_full_id() {
    let prefix = DeviceId::parse_user_id("BAAAAAA").unwrap();
    assert!(prefix.matches_peer(&id_a()));
    assert!(id_a().matches_peer(&prefix));
    let wrong = DeviceId::parse_user_id("BAAAAAB").unwrap();
    assert!(!wrong.matches_peer(&id_a()));
    assert!(!id_a().matches_peer(&wrong));
    assert!(!id_a().matches_peer(&id_b()));
    assert!(id_a().matches_peer(&id_a()));
}

#[test]
fn hex_form_is_lowercase_64_characters() {
    let hex = id_a().to_hex();
    assert_eq!(hex.len(), 64);
    assert!(hex.starts_with("08") && hex[2..].chars().all(|c| c == '0'));
    assert_eq!(DeviceId::from_hex(&hex), Some(id_a()));
    assert_eq!(DeviceId::from_hex(&format!(" {hex} ")), Some(id_a()));
    assert_eq!(DeviceId::from_hex(&hex[..63]), None);
}
