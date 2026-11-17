//! Text and buffer of the notification raised for new folder offers.

use syncthing_vita_worker::defaults::{
    NOTIFICATION_BUFFER_UNITS, NOTIFICATION_NAME_CHARS, NOTIFICATION_TEXT_UNITS,
};
use syncthing_vita_worker::notification::{notification_text, utf16_buffer, NotifyLanguage};
use syncthing_vita_worker::offers::NewOffer;

fn offer(device: &str, label: &str) -> NewOffer {
    NewOffer {
        device_name: device.to_owned(),
        folder_id: "id".to_owned(),
        label: label.to_owned(),
    }
}

#[test]
fn test_language_codes() {
    use NotifyLanguage::*;
    assert_eq!(NotifyLanguage::from_system_code(Some(2)), French);
    assert_eq!(NotifyLanguage::from_system_code(Some(3)), Spanish);
    assert_eq!(NotifyLanguage::from_system_code(Some(4)), German);
    assert_eq!(NotifyLanguage::from_system_code(Some(5)), Italian);
    for other in [Some(0), Some(1), Some(6), Some(18), Some(-1), None] {
        assert_eq!(NotifyLanguage::from_system_code(other), English);
    }
}

#[test]
fn test_texts_in_every_language() {
    let one = [offer("Desktop", "Saves")];
    let two = [offer("A", "x"), offer("B", "y")];
    let cases = [
        (
            NotifyLanguage::English,
            "Syncthing: Desktop wants to share \"Saves\". Open Syncthing to accept.",
            "Syncthing: 2 new shared folders are waiting. Open Syncthing to accept.",
        ),
        (
            NotifyLanguage::French,
            "Syncthing : Desktop veut partager « Saves ». Ouvrez Syncthing pour accepter.",
            "Syncthing : 2 nouveaux dossiers partagés en attente. Ouvrez Syncthing pour accepter.",
        ),
        (
            NotifyLanguage::Spanish,
            "Syncthing: Desktop quiere compartir «Saves». Abre Syncthing para aceptar.",
            "Syncthing: 2 carpetas compartidas nuevas en espera. Abre Syncthing para aceptar.",
        ),
        (
            NotifyLanguage::German,
            "Syncthing: Desktop möchte „Saves“ teilen. Öffne Syncthing zum Annehmen.",
            "Syncthing: 2 neue geteilte Ordner warten. Öffne Syncthing zum Annehmen.",
        ),
        (
            NotifyLanguage::Italian,
            "Syncthing: Desktop vuole condividere \"Saves\". Apri Syncthing per accettare.",
            "Syncthing: 2 nuove cartelle condivise in attesa. Apri Syncthing per accettare.",
        ),
    ];
    for (lang, single, several) in cases {
        assert_eq!(notification_text(lang, &one).as_deref(), Some(single));
        assert_eq!(notification_text(lang, &two).as_deref(), Some(several));
        assert_eq!(notification_text(lang, &[]), None);
        // Every text fits the buffer.
        assert!(single.encode_utf16().count() <= NOTIFICATION_TEXT_UNITS);
        assert!(several.encode_utf16().count() <= NOTIFICATION_TEXT_UNITS);
    }
}

#[test]
fn test_peer_text_is_cleaned_and_shortened() {
    let exact = "n".repeat(NOTIFICATION_NAME_CHARS);
    let long = "l".repeat(NOTIFICATION_NAME_CHARS + 1);
    let text = notification_text(
        NotifyLanguage::English,
        &[offer(&format!("Bad\n\u{1b}Name{exact}"), &long)],
    )
    .unwrap();
    assert!(!text.chars().any(char::is_control), "{text:?}");
    let text = notification_text(NotifyLanguage::English, &[offer(&exact, &long)]).unwrap();
    let cut = format!("{}…", "l".repeat(NOTIFICATION_NAME_CHARS - 1));
    assert_eq!(
        text,
        format!("Syncthing: {exact} wants to share \"{cut}\". Open Syncthing to accept.")
    );
}

#[test]
fn test_utf16_buffer() {
    assert_eq!(NOTIFICATION_BUFFER_UNITS * 2, 0x410);
    let buf = utf16_buffer("Hé😀");
    assert_eq!(&buf[..5], &[0x48, 0xE9, 0xD83D, 0xDE00, 0]);
    assert!(buf[4..].iter().all(|&u| u == 0));
    // Cut at the limit, never inside a surrogate pair, always NUL-terminated.
    let at_limit = "a".repeat(NOTIFICATION_TEXT_UNITS);
    let buf = utf16_buffer(&format!("{at_limit}b"));
    assert_eq!(buf[NOTIFICATION_TEXT_UNITS - 1], u16::from(b'a'));
    assert_eq!(buf[NOTIFICATION_TEXT_UNITS], 0);
    let almost = "a".repeat(NOTIFICATION_TEXT_UNITS - 1);
    let buf = utf16_buffer(&format!("{almost}😀"));
    assert_eq!(
        buf[NOTIFICATION_TEXT_UNITS - 1],
        0,
        "the pair does not fit: dropped"
    );
    assert_eq!(*buf.last().unwrap(), 0);
}
