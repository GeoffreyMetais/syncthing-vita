//! `UiText`: messages chosen by the domain / application layers are rendered in every language,
//! with all `{placeholders}` filled.

use syncthing_core::DeviceIdParseError;
use vita_syncthing_app::domain::{Language, UiText};
use vita_syncthing_app::ui::strings;

fn s(v: &str) -> String {
    v.to_owned()
}

/// One sample of every `UiText` variant (except `Raw`).
fn all_messages() -> Vec<UiText> {
    vec![
        UiText::ReadyPressCross,
        UiText::ReadyManualPairing,
        UiText::NoPeerLinked,
        UiText::SelectDeviceToConnect,
        UiText::NoFoldersSynced,
        UiText::NoRemoteFolders,
        UiText::StartingSync,
        UiText::ScanningLan,
        UiText::ConnectingTo {
            name: s("PC"),
            addr: s("192.168.1.2:22000"),
        },
        UiText::ExchangingIndex,
        UiText::FileUpToDate { file: s("a.sav") },
        UiText::Downloading {
            file: s("a.sav"),
            pct: 42,
        },
        UiText::IndexPublished { files: 7 },
        UiText::Uploading { file: s("a.sav") },
        UiText::SyncFinished,
        UiText::Connections { count: 1 },
        UiText::Connections { count: 3 },
        UiText::SyncSummary {
            connections: 3,
            up_to_date: 4,
            downloaded: 5,
            downloaded_kb: 6,
            uploaded: 7,
            uploaded_bytes: 8,
        },
        UiText::FoundLanPeers { count: 2 },
        UiText::NoPeerAddDevice,
        UiText::DiscoveryError {
            error: s("timeout"),
        },
        UiText::SyncingWithPeers,
        UiText::ThemeDark,
        UiText::ThemeOled,
        UiText::LanguageChanged(Language::German),
        UiText::DeviceLinked {
            device: s("PC"),
            share: s("Saves"),
        },
        UiText::DeviceUnlinked {
            device: s("PC"),
            share: s("Saves"),
        },
        UiText::LinkDeviceFirst,
        UiText::AddedShortId {
            prefix: s("MFZWI3D"),
        },
        UiText::AddedDevice {
            name: s("PC"),
            short: s("MFZWI3D"),
        },
        UiText::OwnDeviceId,
        UiText::InvalidDeviceId(DeviceIdParseError::TooLong(60)),
        UiText::QrNotDeviceId,
        UiText::ShareDeleted { label: s("Saves") },
        UiText::LinkDeviceToShare { label: s("Saves") },
        UiText::ShareCreated { label: s("Saves") },
        UiText::ShareNotCreated { label: s("Saves") },
        UiText::SavedIn {
            path: s("ux0:music"),
        },
        UiText::ShareActive { id: s("saves") },
        UiText::SharePaused { id: s("saves") },
        UiText::OfferAccepted {
            label: s("Saves"),
            id: s("saves"),
        },
        UiText::ChooseWhereToStore { label: s("Saves") },
        UiText::OfferDeclined {
            label: s("Saves"),
            id: s("saves"),
        },
        UiText::NoDeclinedFolders,
        UiText::DeclinedCleared { count: 2 },
        UiText::NoDevicesLinked,
        UiText::SharedWith {
            devices: s("PC (MFZWI3D)"),
        },
        UiText::PickerShareThisFolder { path: s("ux0:") },
        UiText::PickerKeepCurrent { path: s("ux0:") },
        UiText::PickerSaveData { path: s("ux0:") },
        UiText::PickerPictures { path: s("ux0:") },
        UiText::PickerMusic { path: s("ux0:") },
        UiText::PickerRetroArch { path: s("ux0:") },
        UiText::PickerDefaultFolder { path: s("ux0:") },
        UiText::PickerBrowsedFolder { path: s("ux0:") },
    ]
}

#[test]
fn test_every_message_is_rendered_in_every_language() {
    for lang in Language::ALL {
        let catalog = strings::strings(lang);
        for msg in all_messages() {
            let text = catalog.text(&msg);
            assert!(!text.trim().is_empty(), "{lang:?} {msg:?} is empty");
            assert!(
                !text.contains('{') && !text.contains('}'),
                "{lang:?} {msg:?} has an unfilled placeholder: {text}"
            );
        }
    }
}

#[test]
fn test_messages_are_translated_and_parameters_filled() {
    let en = strings::strings(Language::English);
    let fr = strings::strings(Language::French);
    let de = strings::strings(Language::German);

    assert_eq!(
        en.text(&UiText::SelectDeviceToConnect),
        "Select or add a device in a share's Devices to connect"
    );
    assert_eq!(fr.text(&UiText::NoPeerLinked), "Aucun pair lié");
    assert_eq!(
        de.text(&UiText::Downloading {
            file: s("a.sav"),
            pct: 42
        }),
        "Lade a.sav herunter (42 %)"
    );
    assert_eq!(
        en.text(&UiText::SyncSummary {
            connections: 3,
            up_to_date: 4,
            downloaded: 5,
            downloaded_kb: 6,
            uploaded: 7,
            uploaded_bytes: 8,
        }),
        "Connected (1+2 connections) · 4 up to date | 5 DL (6 KB) | 7 UL (8 B)"
    );
    assert_eq!(
        fr.text(&UiText::LanguageChanged(Language::Italian)),
        "Langue : Italiano"
    );
    assert_eq!(
        fr.text(&UiText::PickerShareThisFolder { path: s("ux0:") }),
        "Partager ce dossier (ux0:)"
    );
    // Data is shown as-is in every language
    assert_eq!(fr.text(&UiText::raw("picture/")), "picture/");

    // Every parameterless message differs from English in the other languages
    for lang in [
        Language::French,
        Language::Spanish,
        Language::German,
        Language::Italian,
    ] {
        let other = strings::strings(lang);
        for msg in [
            UiText::ReadyPressCross,
            UiText::NoFoldersSynced,
            UiText::SyncFinished,
            UiText::LinkDeviceFirst,
            UiText::QrNotDeviceId,
            UiText::NoDevicesLinked,
        ] {
            assert_ne!(other.text(&msg), en.text(&msg), "{lang:?} {msg:?}");
        }
    }
}

#[test]
fn test_device_id_errors_are_translated() {
    let en = strings::strings(Language::English);
    let it = strings::strings(Language::Italian);
    let errors = [
        DeviceIdParseError::Empty,
        DeviceIdParseError::InvalidChar('!'),
        DeviceIdParseError::TooShort,
        DeviceIdParseError::TooLong(60),
        DeviceIdParseError::ChecksumMismatch { first_group: 3 },
        DeviceIdParseError::DecodeFailed,
    ];
    for err in errors {
        assert_ne!(it.device_id_error(err), en.device_id_error(err));
    }
    assert_eq!(
        en.device_id_error(DeviceIdParseError::ChecksumMismatch { first_group: 3 }),
        "Typo in groups 3-4 (check digit mismatch)"
    );
    assert_eq!(
        it.device_id_error(DeviceIdParseError::InvalidChar('!')),
        "Carattere non valido '!' (Base32: A-Z, 2-7)"
    );
}
