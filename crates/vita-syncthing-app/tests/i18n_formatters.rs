//! Exact output of the dynamic formatters of `ui::i18n::Strings` in all five languages, of the
//! `{placeholder}` filler and of the `Messages` templates of parameterised `UiText` values.

use syncthing_core::DeviceIdParseError;
use vita_syncthing_app::domain::{Language, UiText};
use vita_syncthing_app::ui::i18n::messages::fill;
use vita_syncthing_app::ui::i18n::{strings, Strings};

const ALL: [Language; 5] = [
    Language::English,
    Language::French,
    Language::Spanish,
    Language::German,
    Language::Italian,
];

fn cat(lang: Language) -> &'static Strings {
    strings(lang)
}

#[test]
fn test_fill_replaces_every_occurrence_and_keeps_unknown_keys() {
    assert_eq!(fill("{a}-{a}-{b}", &[("a", &1), ("b", &"x")]), "1-1-x");
    assert_eq!(fill("{a}-{c}", &[("a", &"ok")]), "ok-{c}");
    assert_eq!(fill("no placeholders", &[("a", &1)]), "no placeholders");
    assert_eq!(fill("{a}{a}", &[]), "{a}{a}");
    // Values are inserted as is, even when they look like placeholders.
    assert_eq!(fill("[{a}]", &[("a", &"{b}")]), "[{b}]");
}

#[test]
fn test_catalog_reports_its_language() {
    for lang in ALL {
        assert_eq!(strings(lang).language, lang, "catalog of {lang:?}");
    }
}

#[test]
fn test_overview_subtitle_in_every_language() {
    let expected = [
        "5 active share(s) · 7 known device(s)",
        "5 partage(s) actif(s) · 7 appareil(s) connu(s)",
        "5 carpeta(s) activa(s) · 7 dispositivo(s) conocido(s)",
        "5 aktive Freigabe(n) · 7 bekannte(s) Gerät(e)",
        "5 condivisione/i attiva/e · 7 dispositivo/i noto/i",
    ];
    for (lang, want) in ALL.into_iter().zip(expected) {
        assert_eq!(cat(lang).fmt_active_shares_devices(5, 7), want, "{lang:?}");
    }
}

#[test]
fn test_template_subtitle_and_picker_target_in_every_language() {
    let template = [
        "ID: sc  ·  Path: ux0:picture",
        "ID : sc  ·  Chemin : ux0:picture",
        "ID: sc  ·  Ruta: ux0:picture",
        "ID: sc  ·  Pfad: ux0:picture",
        "ID: sc  ·  Percorso: ux0:picture",
    ];
    let picker = [
        "For Shots (sc)",
        "Pour Shots (sc)",
        "Para Shots (sc)",
        "Für Shots (sc)",
        "Per Shots (sc)",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(
            cat(lang).fmt_template_sub("sc", "ux0:picture"),
            template[i],
            "{lang:?}"
        );
        assert_eq!(
            cat(lang).fmt_picker_target("Shots", "sc"),
            picker[i],
            "{lang:?}"
        );
    }
}

#[test]
fn test_declined_summary_in_every_language() {
    let expected = [
        "2 declined: a, b",
        "2 refusé(s) : a, b",
        "2 rechazado(s): a, b",
        "2 abgelehnt: a, b",
        "2 rifiutato/i: a, b",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(
            cat(lang).fmt_declined_summary(2, "a, b"),
            expected[i],
            "{lang:?}"
        );
    }
}

#[test]
fn test_files_kept_message_in_every_language() {
    let expected = [
        "Files in ux0:music are kept on the PS Vita.",
        "Les fichiers dans ux0:music sont conservés sur la PS Vita.",
        "Los archivos en ux0:music se conservan en la PS Vita.",
        "Dateien in ux0:music verbleiben auf der PS Vita.",
        "I file in ux0:music sono conservati sulla PS Vita.",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(
            cat(lang).fmt_files_kept_on_vita("ux0:music"),
            expected[i],
            "{lang:?}"
        );
    }
}

#[test]
fn test_devices_title_and_folder_offer_in_every_language() {
    let devices = [
        "Devices for Photos",
        "Appareils pour Photos",
        "Dispositivos para Photos",
        "Geräte für Photos",
        "Dispositivi per Photos",
    ];
    let offer = [
        "NAS wants to share a folder with this PS Vita",
        "NAS souhaite partager un dossier avec cette PS Vita",
        "NAS quiere compartir una carpeta con esta PS Vita",
        "NAS möchte einen Ordner mit dieser PS Vita teilen",
        "NAS vuole condividere una cartella con questa PS Vita",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(
            cat(lang).fmt_devices_for_share("Photos"),
            devices[i],
            "{lang:?}"
        );
        assert_eq!(cat(lang).fmt_folder_offer_sub("NAS"), offer[i], "{lang:?}");
    }
}

#[test]
fn test_offers_waiting_in_every_language() {
    let expected = [
        "3 more offer(s) waiting",
        "3 autre(s) offre(s) en attente",
        "3 oferta(s) más en espera",
        "3 weitere(s) Angebot(e) ausstehend",
        "3 ulteriore/i offerta/e in attesa",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(cat(lang).fmt_offers_waiting(3), expected[i], "{lang:?}");
    }
}

#[test]
fn test_screensaver_syncing_line_with_and_without_percentage() {
    let with_pct = [
        "Syncing 42% · a.txt",
        "Synchronisation 42% · a.txt",
        "Sincronizando 42% · a.txt",
        "Synchronisiere 42% · a.txt",
        "Sincronizzazione 42% · a.txt",
    ];
    let without = [
        "Syncing · a.txt",
        "Synchronisation · a.txt",
        "Sincronizando · a.txt",
        "Synchronisiere · a.txt",
        "Sincronizzazione · a.txt",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(
            cat(lang).fmt_saver_syncing(Some(42), "a.txt"),
            with_pct[i],
            "{lang:?}"
        );
        assert_eq!(
            cat(lang).fmt_saver_syncing(None, "a.txt"),
            without[i],
            "{lang:?}"
        );
    }
    // Zero per cent is still a percentage.
    assert_eq!(
        cat(Language::English).fmt_saver_syncing(Some(0), "f"),
        "Syncing 0% · f"
    );
}

#[test]
fn test_screensaver_uploading_line_in_every_language() {
    let expected = [
        "Uploading · a.txt",
        "Envoi · a.txt",
        "Enviando · a.txt",
        "Sende · a.txt",
        "Invio · a.txt",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(
            cat(lang).fmt_saver_uploading("a.txt"),
            expected[i],
            "{lang:?}"
        );
    }
}

#[test]
fn test_log_range_and_item_counter_are_language_neutral() {
    for lang in ALL {
        assert_eq!(
            cat(lang).fmt_log_range(1, 20, 300),
            "1–20 / 300",
            "{lang:?}"
        );
        assert_eq!(cat(lang).fmt_item_counter(2, 9), "2 / 9", "{lang:?}");
    }
}

#[test]
fn test_short_id_label_in_every_language() {
    let expected = [
        "Short ID: ABC",
        "ID court : ABC",
        "ID corto: ABC",
        "Kurz-ID: ABC",
        "ID breve: ABC",
    ];
    for (i, lang) in ALL.into_iter().enumerate() {
        assert_eq!(cat(lang).fmt_short_id_label("ABC"), expected[i], "{lang:?}");
    }
}

#[test]
fn test_share_title_method_in_every_language() {
    for lang in ALL {
        assert_eq!(
            cat(lang).fmt_share_title("Photos", "pics"),
            "Photos  ·  pics",
            "{lang:?}"
        );
    }
}

#[test]
fn test_connections_message_counts_secondary_connections() {
    let en = cat(Language::English);
    assert_eq!(
        en.text(&UiText::Connections { count: 1 }),
        "Connected (1 connection)"
    );
    assert_eq!(
        en.text(&UiText::Connections { count: 3 }),
        "Connected (1+2 connections)"
    );
    assert_eq!(
        cat(Language::French).text(&UiText::Connections { count: 2 }),
        "Connecté (1+1 connexions)"
    );
}

#[test]
fn test_sync_summary_fills_every_placeholder() {
    let summary = UiText::SyncSummary {
        connections: 3,
        up_to_date: 5,
        downloaded: 2,
        downloaded_kb: 10,
        uploaded: 1,
        uploaded_bytes: 2048,
    };
    assert_eq!(
        cat(Language::English).text(&summary),
        "Connected (1+2 connections) · 5 up to date | 2 DL (10 KB) | 1 UL (2048 B)"
    );
    assert_eq!(
        cat(Language::French).text(&summary),
        "Connecté (1+2 connexions) · 5 à jour | 2 reçus (10 Ko) | 1 envoyés (2048 o)"
    );
}

#[test]
fn test_parameterised_messages_fill_their_placeholders_in_english() {
    let en = cat(Language::English);
    assert_eq!(
        en.text(&UiText::ConnectingTo {
            name: "NAS".into(),
            addr: "10.0.0.2:22000".into()
        }),
        "Connecting over TLS 1.3 to NAS (10.0.0.2:22000)..."
    );
    assert_eq!(
        en.text(&UiText::Downloading {
            file: "a.bin".into(),
            pct: 50
        }),
        "Downloading a.bin (50%)"
    );
    assert_eq!(
        en.text(&UiText::IndexPublished { files: 12 }),
        "Local Vita index published (12 files)"
    );
    assert_eq!(
        en.text(&UiText::FoundLanPeers { count: 2 }).as_ref(),
        "Found 2 LAN peer(s). Select a device in a share's Devices to connect."
    );
    assert_eq!(
        en.text(&UiText::LocalChangesReverted {
            id: "pics".into(),
            count: 4
        })
        .as_ref(),
        en.messages
            .local_changes_reverted
            .replace("{id}", "pics")
            .replace("{count}", "4")
    );
    assert_eq!(
        en.text(&UiText::LanguageChanged(Language::German)),
        "Language: Deutsch"
    );
    assert_eq!(
        en.text(&UiText::DeclinedCleared { count: 0 }).as_ref(),
        en.messages.declined_cleared.replace("{count}", "0")
    );
}

#[test]
fn test_device_id_errors_render_their_numbers_in_every_language() {
    for lang in ALL {
        let c = cat(lang);
        let checksum = c.device_id_error(DeviceIdParseError::ChecksumMismatch { first_group: 3 });
        assert!(
            checksum.contains('3') && checksum.contains('4'),
            "{lang:?}: {checksum}"
        );
        let long = c.device_id_error(DeviceIdParseError::TooLong(61));
        assert!(long.contains("61"), "{lang:?}: {long}");
        let bad = c.device_id_error(DeviceIdParseError::InvalidChar('8'));
        assert!(bad.contains('8'), "{lang:?}: {bad}");
    }
    let en = cat(Language::English);
    assert_eq!(
        en.device_id_error(DeviceIdParseError::Empty),
        "Device ID cannot be empty"
    );
    assert_eq!(
        en.device_id_error(DeviceIdParseError::TooLong(61)),
        "Device ID is too long (61 characters, max 56)"
    );
    assert_eq!(
        en.device_id_error(DeviceIdParseError::ChecksumMismatch { first_group: 3 }),
        "Typo in groups 3-4 (check digit mismatch)"
    );
    assert_eq!(
        en.device_id_error(DeviceIdParseError::InvalidChar('8')),
        "Invalid character '8' (use Base32 A-Z, 2-7)"
    );
    assert_eq!(
        en.device_id_error(DeviceIdParseError::DecodeFailed),
        "Device ID could not be decoded"
    );
}
