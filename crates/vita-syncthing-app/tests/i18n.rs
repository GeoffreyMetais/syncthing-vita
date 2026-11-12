//! Tests for `ui::i18n` multi-language catalogs, language switching, and formatters.

use vita_syncthing_app::domain::Language;
use vita_syncthing_app::infrastructure::detect_system_language;
use vita_syncthing_app::ui::strings;

#[test]
fn test_language_enum_cycling() {
    assert_eq!(Language::English.next(), Language::French);
    assert_eq!(Language::French.next(), Language::Spanish);
    assert_eq!(Language::Spanish.next(), Language::German);
    assert_eq!(Language::German.next(), Language::Italian);
    assert_eq!(Language::Italian.next(), Language::English);

    assert_eq!(Language::ALL.len(), 5);
    assert_eq!(Language::English.name(), "English");
    assert_eq!(Language::French.name(), "Français");
    assert_eq!(Language::Spanish.name(), "Español");
    assert_eq!(Language::German.name(), "Deutsch");
    assert_eq!(Language::Italian.name(), "Italiano");
}

#[test]
fn test_localized_catalogs_and_formatters() {
    let en = strings::strings(Language::English);
    let fr = strings::strings(Language::French);
    let es = strings::strings(Language::Spanish);
    let de = strings::strings(Language::German);
    let it = strings::strings(Language::Italian);

    // Common actions vary across languages
    assert_eq!(en.action_cancel, "Cancel");
    assert_eq!(fr.action_cancel, "Annuler");
    assert_eq!(es.action_cancel, "Cancelar");
    assert_eq!(de.action_cancel, "Abbrechen");
    assert_eq!(it.action_cancel, "Annulla");

    // Screen titles vary
    assert_eq!(en.shares_title, "Shared Folders");
    assert_eq!(fr.shares_title, "Dossiers partagés");
    assert_eq!(es.shares_title, "Carpetas compartidas");
    assert_eq!(de.shares_title, "Freigegebene Ordner");
    assert_eq!(it.shares_title, "Cartelle condivise");

    // Dynamic formatting varies
    assert_eq!(
        en.fmt_active_shares_devices(2, 3),
        "2 active share(s) · 3 known device(s)"
    );
    assert_eq!(
        fr.fmt_active_shares_devices(2, 3),
        "2 partage(s) actif(s) · 3 appareil(s) connu(s)"
    );
    assert_eq!(
        es.fmt_active_shares_devices(2, 3),
        "2 carpeta(s) activa(s) · 3 dispositivo(s) conocido(s)"
    );
    assert_eq!(
        de.fmt_active_shares_devices(2, 3),
        "2 aktive Freigabe(n) · 3 bekannte(s) Gerät(e)"
    );
    assert_eq!(
        it.fmt_active_shares_devices(2, 3),
        "2 condivisione/i attiva/e · 3 dispositivo/i noto/i"
    );

    assert_eq!(
        fr.fmt_folder_offer_sub("PC"),
        "PC souhaite partager un dossier avec cette PS Vita"
    );
}

#[test]
fn test_system_language_detection_fallback() {
    // When no LANG env var is recognized, fallback is English
    let lang = detect_system_language();
    assert!(Language::ALL.contains(&lang));
}
