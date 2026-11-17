//! The PS Vita notification raised when a cycle recorded new folder offers: what it says, in
//! which language, and its UTF-16 buffer. Pure logic; `vita/notify.rs` only sends it.
//!
//! One notification per cycle at most, and only for offers recorded during that cycle: an offer
//! still pending from an earlier cycle never notifies again.

use crate::defaults::{
    NOTIFICATION_BUFFER_UNITS, NOTIFICATION_NAME_CHARS, NOTIFICATION_TEXT_UNITS,
};
use crate::offers::NewOffer;

/// Languages of the notification (the app's languages); English for any other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyLanguage {
    English,
    French,
    Spanish,
    German,
    Italian,
}

impl NotifyLanguage {
    /// From the system language code (`SCE_SYSTEM_PARAM_LANG_*`, registry key
    /// `/CONFIG/SYSTEM/language`), English when unknown or unreadable.
    pub fn from_system_code(code: Option<i32>) -> Self {
        match code {
            Some(2) => Self::French,
            Some(3) => Self::Spanish,
            Some(4) => Self::German,
            Some(5) => Self::Italian,
            _ => Self::English,
        }
    }
}

/// Text of the notification for the offers recorded by a cycle, `None` without any.
pub fn notification_text(lang: NotifyLanguage, new_offers: &[NewOffer]) -> Option<String> {
    let text = match new_offers {
        [] => return None,
        [offer] => {
            let device = shorten(&offer.device_name);
            let label = shorten(&offer.label);
            match lang {
                NotifyLanguage::English => format!(
                    "Syncthing: {device} wants to share \"{label}\". Open Syncthing to accept."
                ),
                NotifyLanguage::French => format!(
                    "Syncthing : {device} veut partager « {label} ». Ouvrez Syncthing pour accepter."
                ),
                NotifyLanguage::Spanish => format!(
                    "Syncthing: {device} quiere compartir «{label}». Abre Syncthing para aceptar."
                ),
                NotifyLanguage::German => format!(
                    "Syncthing: {device} möchte „{label}“ teilen. Öffne Syncthing zum Annehmen."
                ),
                NotifyLanguage::Italian => format!(
                    "Syncthing: {device} vuole condividere \"{label}\". Apri Syncthing per accettare."
                ),
            }
        }
        several => {
            let n = several.len();
            match lang {
                NotifyLanguage::English => format!(
                    "Syncthing: {n} new shared folders are waiting. Open Syncthing to accept."
                ),
                NotifyLanguage::French => format!(
                    "Syncthing : {n} nouveaux dossiers partagés en attente. Ouvrez Syncthing pour accepter."
                ),
                NotifyLanguage::Spanish => format!(
                    "Syncthing: {n} carpetas compartidas nuevas en espera. Abre Syncthing para aceptar."
                ),
                NotifyLanguage::German => format!(
                    "Syncthing: {n} neue geteilte Ordner warten. Öffne Syncthing zum Annehmen."
                ),
                NotifyLanguage::Italian => format!(
                    "Syncthing: {n} nuove cartelle condivise in attesa. Apri Syncthing per accettare."
                ),
            }
        }
    };
    Some(text)
}

/// Peer-supplied text in the notification: no control characters, at most
/// `NOTIFICATION_NAME_CHARS` characters (an ellipsis marks a cut).
fn shorten(text: &str) -> String {
    let clean: Vec<char> = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if clean.len() <= NOTIFICATION_NAME_CHARS {
        return clean.into_iter().collect();
    }
    let mut cut: String = clean[..NOTIFICATION_NAME_CHARS - 1].iter().collect();
    cut.push('…');
    cut
}

/// The buffer `sceNotificationUtilSendNotification` reads (0x410 bytes of UTF-16, NUL
/// terminated): `text` cut at `NOTIFICATION_TEXT_UNITS` units, never inside a surrogate pair.
pub fn utf16_buffer(text: &str) -> [u16; NOTIFICATION_BUFFER_UNITS] {
    let mut buf = [0u16; NOTIFICATION_BUFFER_UNITS];
    let mut len = 0;
    for ch in text.chars() {
        let mut units = [0u16; 2];
        let encoded = ch.encode_utf16(&mut units);
        if len + encoded.len() > NOTIFICATION_TEXT_UNITS {
            break;
        }
        buf[len..len + encoded.len()].copy_from_slice(encoded);
        len += encoded.len();
    }
    buf
}
