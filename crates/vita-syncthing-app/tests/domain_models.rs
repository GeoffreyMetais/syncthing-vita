//! Value objects of `domain/models.rs` and `domain/ui_text.rs`: languages, theme and share
//! modes, camera preview, byte formatting, keypad layout and text constructors.

use syncthing_core::DeviceId;
use vita_syncthing_app::domain::{
    fmt_size, ActiveTab, CameraPreview, Language, LanguagePreference, PickerMode, ShareConfigEntry,
    ShareMode, ThemeMode, UiText, ADD_DEVICE_KEYPAD, BROWSER_ROWS, BROWSE_TEMPLATE_ID,
    NEW_SHARE_TEMPLATES, VISIBLE_DEVICES,
};

const A: DeviceId = DeviceId([0x41; 32]);

#[test]
fn test_language_order_names_and_default() {
    assert_eq!(Language::default(), Language::English);
    assert_eq!(
        Language::ALL,
        [
            Language::English,
            Language::French,
            Language::Spanish,
            Language::German,
            Language::Italian
        ]
    );
    let names: Vec<&str> = Language::ALL.iter().map(|l| l.name()).collect();
    assert_eq!(
        names,
        ["English", "Français", "Español", "Deutsch", "Italiano"]
    );
}

#[test]
fn test_language_codes_round_trip() {
    let codes: Vec<&str> = Language::ALL.iter().map(|l| l.code()).collect();
    assert_eq!(codes, ["en", "fr", "es", "de", "it"]);
    for lang in Language::ALL {
        assert_eq!(Language::from_code(lang.code()), Some(lang));
    }
    assert_eq!(Language::from_code("  FR "), Some(Language::French));
    assert_eq!(Language::from_code("xx"), None);
    assert_eq!(Language::from_code(""), None);
}

#[test]
fn test_language_next_cycles_through_all_five() {
    let mut lang = Language::English;
    let mut seen = Vec::new();
    for _ in 0..5 {
        seen.push(lang);
        lang = lang.next();
    }
    assert_eq!(seen, Language::ALL.to_vec());
    assert_eq!(lang, Language::English);
    assert_eq!(Language::Italian.next(), Language::English);
}

#[test]
fn test_language_preference_codes_and_parsing() {
    assert_eq!(LanguagePreference::default(), LanguagePreference::System);
    assert_eq!(LanguagePreference::System.as_str(), "system");
    assert_eq!(LanguagePreference::Fixed(Language::German).as_str(), "de");
    assert_eq!(
        LanguagePreference::parse(" SYSTEM "),
        Some(LanguagePreference::System)
    );
    assert_eq!(
        LanguagePreference::parse("de"),
        Some(LanguagePreference::Fixed(Language::German))
    );
    assert_eq!(LanguagePreference::parse("klingon"), None);
    assert_eq!(LanguagePreference::parse(""), None);
}

#[test]
fn test_language_preference_next_goes_system_then_every_language() {
    let mut pref = LanguagePreference::System;
    let mut seen = vec![pref];
    for _ in 0..6 {
        pref = pref.next();
        seen.push(pref);
    }
    let expected: Vec<LanguagePreference> = std::iter::once(LanguagePreference::System)
        .chain(Language::ALL.into_iter().map(LanguagePreference::Fixed))
        .chain(std::iter::once(LanguagePreference::System))
        .collect();
    assert_eq!(seen, expected);
}

#[test]
fn test_language_preference_resolves_system_to_the_detected_language() {
    assert_eq!(
        LanguagePreference::System.resolve(Language::Spanish),
        Language::Spanish
    );
    assert_eq!(
        LanguagePreference::Fixed(Language::Italian).resolve(Language::Spanish),
        Language::Italian
    );
}

#[test]
fn test_theme_mode_default_codes_and_parsing() {
    assert_eq!(ThemeMode::default(), ThemeMode::Oled);
    assert_eq!(ThemeMode::Dark.as_str(), "dark");
    assert_eq!(ThemeMode::Oled.as_str(), "oled");
    assert_eq!(ThemeMode::parse("DARK"), Some(ThemeMode::Dark));
    assert_eq!(ThemeMode::parse(" 0 "), Some(ThemeMode::Dark));
    assert_eq!(ThemeMode::parse("oled"), Some(ThemeMode::Oled));
    assert_eq!(ThemeMode::parse("1"), Some(ThemeMode::Oled));
    assert_eq!(ThemeMode::parse("light"), None);
    assert_eq!(ThemeMode::parse("2"), None);
}

#[test]
fn test_active_tab_discriminants_match_the_sidebar_order() {
    assert_eq!(ActiveTab::Dashboard as u8, 0);
    assert_eq!(ActiveTab::FolderPicker as u8, 1);
    assert_eq!(ActiveTab::Settings as u8, 2);
    assert_ne!(PickerMode::AcceptedShareTarget, PickerMode::CreateNewShare);
}

#[test]
fn test_share_mode_default_toggle_and_codes() {
    assert_eq!(ShareMode::default(), ShareMode::ReadWrite);
    assert_eq!(ShareMode::ReadWrite.toggled(), ShareMode::ReadOnly);
    assert_eq!(ShareMode::ReadOnly.toggled(), ShareMode::SendOnly);
    assert_eq!(ShareMode::SendOnly.toggled(), ShareMode::ReadWrite);
    assert_eq!(ShareMode::ReadWrite.code(), "rw");
    assert_eq!(ShareMode::ReadOnly.code(), "ro");
    assert_eq!(ShareMode::SendOnly.code(), "so");
    assert_eq!(ShareMode::from_code("ro"), ShareMode::ReadOnly);
    assert_eq!(ShareMode::from_code(" ro "), ShareMode::ReadOnly);
    assert_eq!(ShareMode::from_code("so"), ShareMode::SendOnly);
    assert_eq!(ShareMode::from_code(" so "), ShareMode::SendOnly);
    assert_eq!(ShareMode::from_code("rw"), ShareMode::ReadWrite);
    // Missing or unknown codes are read & write (legacy `shares.cfg` lines).
    assert_eq!(ShareMode::from_code(""), ShareMode::ReadWrite);
    assert_eq!(ShareMode::from_code("RO"), ShareMode::ReadWrite);
}

#[test]
fn test_share_is_linked_to_full_and_placeholder_ids() {
    let full_a = A;
    let placeholder = DeviceId::parse_user_id(&A.short()).unwrap();
    let other = DeviceId([0x42; 32]);
    let share = |devices: Vec<DeviceId>| ShareConfigEntry {
        folder_id: "f".to_owned(),
        label: "f".to_owned(),
        target_path: "ux0:f".to_owned(),
        enabled: true,
        mode: ShareMode::ReadWrite,
        shared_devices: devices,
    };
    assert!(share(vec![full_a]).is_linked_to(&full_a));
    assert!(share(vec![placeholder]).is_linked_to(&full_a));
    assert!(!share(vec![other]).is_linked_to(&full_a));
    assert!(!share(Vec::new()).is_linked_to(&full_a));
}

#[test]
fn test_camera_preview_needs_a_complete_frame() {
    let empty = CameraPreview::default();
    assert!(!empty.has_frame());

    let frame = CameraPreview {
        width: 2,
        height: 2,
        gray: vec![0, 1, 2, 3],
    };
    assert!(frame.has_frame());

    let short = CameraPreview {
        width: 2,
        height: 2,
        gray: vec![0, 1, 2],
    };
    assert!(!short.has_frame());

    let no_height = CameraPreview {
        width: 2,
        height: 0,
        gray: Vec::new(),
    };
    assert!(!no_height.has_frame());
}

#[test]
fn test_camera_preview_needs_a_non_empty_width_and_height() {
    // Zero width with a height and an empty buffer is not a frame, even though 0 == 0 * height.
    let no_width = CameraPreview {
        width: 0,
        height: 2,
        gray: Vec::new(),
    };
    assert!(!no_width.has_frame());
}

#[test]
fn test_camera_preview_frame_length_is_width_times_height() {
    // A 3 x 2 frame has 6 pixels: 3 + 2 = 5 is not the expected length.
    let wide = CameraPreview {
        width: 3,
        height: 2,
        gray: vec![0; 6],
    };
    assert!(wide.has_frame());

    let five = CameraPreview {
        width: 3,
        height: 2,
        gray: vec![0; 5],
    };
    assert!(!five.has_frame());
}

#[test]
fn test_camera_preview_clear_forgets_the_frame() {
    let mut frame = CameraPreview {
        width: 2,
        height: 1,
        gray: vec![7, 8],
    };
    frame.clear();
    assert_eq!(frame, CameraPreview::default());
    assert!(!frame.has_frame());
}

#[test]
fn test_fmt_size_units_and_boundaries() {
    assert_eq!(fmt_size(0), "0 B");
    assert_eq!(fmt_size(1023), "1023 B");
    assert_eq!(fmt_size(1024), "1.0 KB");
    assert_eq!(fmt_size(1536), "1.5 KB");
    assert_eq!(fmt_size(1024 * 1024 - 1), "1024.0 KB");
    assert_eq!(fmt_size(1024 * 1024), "1.0 MB");
    assert_eq!(fmt_size(3 * 1024 * 1024 + 512 * 1024), "3.5 MB");
}

#[test]
fn test_add_device_keypad_layout() {
    assert_eq!(ADD_DEVICE_KEYPAD.len(), 36);
    assert_eq!(ADD_DEVICE_KEYPAD[0], "A");
    assert_eq!(ADD_DEVICE_KEYPAD[25], "Z");
    assert_eq!(ADD_DEVICE_KEYPAD[26], "-");
    assert_eq!(ADD_DEVICE_KEYPAD[27], "2");
    assert_eq!(ADD_DEVICE_KEYPAD[32], "7");
    assert_eq!(ADD_DEVICE_KEYPAD[33], "Bksp");
    assert_eq!(ADD_DEVICE_KEYPAD[34], "Clear");
    assert_eq!(ADD_DEVICE_KEYPAD[35], "Add");
}

#[test]
fn test_list_sizes_and_browse_entry() {
    assert_eq!(VISIBLE_DEVICES, 5);
    assert_eq!(BROWSER_ROWS, 8);
    assert_eq!(BROWSE_TEMPLATE_ID, "__browse__");
    assert_eq!(NEW_SHARE_TEMPLATES.len(), 6);
    let last = NEW_SHARE_TEMPLATES.last().unwrap();
    assert_eq!(last.0, BROWSE_TEMPLATE_ID);
}

#[test]
fn test_ui_text_raw_wraps_data_as_is() {
    assert_eq!(UiText::raw("file.txt"), UiText::Raw("file.txt".to_owned()));
    assert_eq!(UiText::raw(String::from("x")), UiText::Raw("x".to_owned()));
    assert_ne!(UiText::raw("a"), UiText::raw("b"));
}
