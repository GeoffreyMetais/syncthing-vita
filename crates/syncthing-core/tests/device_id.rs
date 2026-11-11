//! `DeviceId` formatting (Luhn mod 32), parsing of user input and short-ID matching.

use syncthing_core::{DeviceId, VitaIdentity};

#[test]
fn test_device_id_formatting() {
    let id = DeviceId([0u8; 32]);
    let formatted = id.to_syncthing_string();
    assert_eq!(formatted.len(), 63);
    assert_eq!(formatted.matches('-').count(), 7);
    assert!(formatted.starts_with("AAAAAAA-AAAAAA"));
}

#[test]
fn test_device_id_parse_full_and_short() {
    let seed = *b"VITA_SYNCTHING_ECDSA_SEED_000001";
    let full_id = VitaIdentity::from_seed(&seed).unwrap().device_id;
    let full_str = full_id.to_syncthing_string();
    let short_str = full_id.short();

    // 1. Full 56-char Syncthing Device ID roundtrip (with hyphens and lowercase)
    let parsed_full = DeviceId::parse_user_id(&full_str.to_lowercase()).unwrap();
    assert_eq!(parsed_full, full_id);
    assert!(!parsed_full.is_short_placeholder());

    // 2. Typo in full 56-char Device ID must be rejected by Luhn mod 32 check
    let mut corrupted = full_str.clone();
    corrupted.replace_range(0..1, if full_str.starts_with('A') { "B" } else { "A" });
    assert!(DeviceId::parse_user_id(&corrupted).is_err());

    // 3. Short 7-char Device ID parsing & peer matching
    let parsed_short = DeviceId::parse_user_id(&short_str).unwrap();
    assert!(parsed_short.is_short_placeholder());
    assert_eq!(parsed_short.short(), short_str);
    assert_eq!(parsed_short.syncthing_prefix(), short_str);
    assert!(parsed_short.matches_peer(&full_id));
    assert!(full_id.matches_peer(&parsed_short));

    // 4. Persistence roundtrip via hex preserves short placeholder & prefix matching
    let reloaded = DeviceId::from_hex(&parsed_short.to_hex()).unwrap();
    assert!(reloaded.is_short_placeholder());
    assert!(reloaded.matches_peer(&full_id));

    // 5. Two-group (14-char) prefix with Luhn check digit
    let two_groups = &full_str[..15]; // "XXXXXXX-XXXXXXX"
    let parsed_14 = DeviceId::parse_user_id(two_groups).unwrap();
    assert!(parsed_14.is_short_placeholder());
    assert!(parsed_14.matches_peer(&full_id));
}
