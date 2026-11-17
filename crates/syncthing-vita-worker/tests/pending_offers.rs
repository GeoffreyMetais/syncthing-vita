//! `pending_offers.cfg`: the offers a background cycle keeps for the app (format shared with
//! the app, escaping, crash-safe replacement, filtering, deduplication, cap, hostile text).

mod common;

use common::{data_dir_with_identity, device, ignored, repo, share};
use syncthing_core::{DeviceId, DeviceIdentity};
use syncthing_vita_worker::config::WorkerConfig;
use syncthing_vita_worker::defaults::{
    MAX_OFFER_FOLDER_ID_BYTES, MAX_OFFER_LABEL_CHARS, MAX_PENDING_OFFERS,
};
use syncthing_vita_worker::offers::{
    clean_label, parse_pending, serialize_pending, NewOffer, OfferBook, OfferVerdict, PendingOffer,
    PENDING_OFFERS_FILE,
};
use vita_syncthing_app::infrastructure::config_fields::{escape_field, unescape_field};

fn id(seed: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[seed; 32]).unwrap().device_id
}

#[test]
fn test_exact_format_and_app_escaping() {
    let a = id(1);
    let offers = vec![
        PendingOffer {
            folder_id: "abcd-1234".to_owned(),
            label: "Saves|100%\nnew".to_owned(),
            device_id: a,
        },
        PendingOffer {
            folder_id: "x|y".to_owned(),
            label: "plain".to_owned(),
            device_id: a,
        },
    ];
    let text = serialize_pending(&offers);
    assert_eq!(
        text,
        format!(
            "abcd-1234|Saves%7C100%25%0Anew|{h}\nx%7Cy|plain|{h}\n",
            h = a.to_hex()
        )
    );
    // Every field is what the app's own escaping produces, and reads back with it.
    for (line, offer) in text.lines().zip(&offers) {
        let parts: Vec<&str> = line.split('|').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], escape_field(&offer.folder_id));
        assert_eq!(parts[1], escape_field(&offer.label));
        assert_eq!(unescape_field(parts[1]), offer.label);
        assert_eq!(DeviceId::from_hex(parts[2]), Some(a));
    }
    assert_eq!(parse_pending(&text), offers);
}

#[test]
fn test_parse_skips_bad_lines() {
    let a = id(2);
    let text = format!(
        "ok|Label|{h}\n\
         two|fields\n\
         four|a|b|{h}\n\
         bad-device|L|zz\n\
         |empty id|{h}\n\
         ctl%0Aid|L|{h}\n\
         crlf|L|{h}\r\n",
        h = a.to_hex()
    );
    let offers = parse_pending(&text);
    let ids: Vec<_> = offers.iter().map(|o| o.folder_id.as_str()).collect();
    assert_eq!(ids, ["ok", "crlf"]);
}

#[test]
fn test_labels_and_folder_ids_are_bounded() {
    assert_eq!(clean_label("  a\u{7}b\nc ", "f"), "a b c");
    assert_eq!(clean_label("", "folder"), "folder");
    assert_eq!(clean_label("\n\t", "folder"), "folder");
    let long: String = "é".repeat(MAX_OFFER_LABEL_CHARS + 10);
    assert_eq!(
        clean_label(&long, "f").chars().count(),
        MAX_OFFER_LABEL_CHARS
    );
    let exact = "a".repeat(MAX_OFFER_LABEL_CHARS);
    assert_eq!(clean_label(&exact, "f"), exact);
    use syncthing_vita_worker::offers::is_valid_folder_id;
    assert!(is_valid_folder_id("a"));
    assert!(is_valid_folder_id(&"a".repeat(MAX_OFFER_FOLDER_ID_BYTES)));
    assert!(!is_valid_folder_id(
        &"a".repeat(MAX_OFFER_FOLDER_ID_BYTES + 1)
    ));
    assert!(!is_valid_folder_id(""));
    assert!(!is_valid_folder_id("a\nb"));
    assert!(!is_valid_folder_id("a\u{1b}[31m"));
}

#[test]
fn test_filtering_dedupe_and_crash_safe_save() {
    let data = data_dir_with_identity("offers_filter", true);
    let (a, b) = (id(3), id(4));
    let app = repo(&data);
    app.save_devices(&[device(a, "A", "dynamic"), device(b, "B", "dynamic")]);
    app.save_shares(&[share("linked", &data.join("l"), true, vec![a])]);
    app.save_ignored_folders(&[ignored("declined", a)]);
    let cfg = WorkerConfig::load(&data).unwrap();

    let mut book = OfferBook::load(&data);
    assert_eq!(
        book.consider(&cfg, a, "A", "linked", "L"),
        OfferVerdict::AlreadyShared
    );
    // The same folder ID shared with another device is still an offer from B.
    assert_eq!(
        book.consider(&cfg, b, "B", "linked", "L"),
        OfferVerdict::Recorded
    );
    assert_eq!(
        book.consider(&cfg, a, "A", "declined", "D"),
        OfferVerdict::Declined
    );
    assert_eq!(
        book.consider(&cfg, b, "B", "declined", "D"),
        OfferVerdict::Recorded
    );
    assert_eq!(
        book.consider(&cfg, a, "A", "new", "New|Label"),
        OfferVerdict::Recorded
    );
    assert_eq!(
        book.consider(&cfg, a, "A", "new", "again"),
        OfferVerdict::AlreadyPending
    );
    assert_eq!(
        book.consider(&cfg, a, "A", "bad\nid", "x"),
        OfferVerdict::Invalid
    );
    assert_eq!(
        book.new_offers()[2],
        NewOffer {
            device_name: "A".to_owned(),
            folder_id: "new".to_owned(),
            label: "New|Label".to_owned(),
        }
    );
    book.save().unwrap();
    let path = data.join(PENDING_OFFERS_FILE);
    let saved = parse_pending(&std::fs::read_to_string(&path).unwrap());
    assert_eq!(saved.len(), 3);
    assert_eq!(saved[2].label, "New|Label");
    assert!(!data.join("pending_offers.cfg.tmp").exists());

    // Next cycle: nothing new, the file is left as is.
    let mut next = OfferBook::load(&data);
    assert_eq!(next.offers().len(), 3);
    assert_eq!(
        next.consider(&cfg, a, "A", "new", "New|Label"),
        OfferVerdict::AlreadyPending
    );
    assert!(next.new_offers().is_empty());
    std::fs::write(&path, "tampered\n").unwrap();
    next.save().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "tampered\n");

    // An interrupted replacement is completed from the backup.
    std::fs::rename(&path, data.join("pending_offers.cfg.bak")).unwrap();
    assert_eq!(
        OfferBook::load(&data).offers().len(),
        0,
        "tampered content is unparseable"
    );
}

#[test]
fn test_cap_drops_further_offers() {
    let data = data_dir_with_identity("offers_cap", true);
    let a = id(5);
    let cfg = WorkerConfig::load(&data).unwrap();
    let mut book = OfferBook::load(&data);
    for i in 0..MAX_PENDING_OFFERS {
        assert_eq!(
            book.consider(&cfg, a, "A", &format!("f{i}"), "L"),
            OfferVerdict::Recorded
        );
    }
    assert!(!book.newly_full());
    assert_eq!(
        book.consider(&cfg, a, "A", "one-more", "L"),
        OfferVerdict::Full
    );
    assert!(
        book.newly_full(),
        "reported by the cycle that filled the file"
    );
    book.save().unwrap();
    let mut next = OfferBook::load(&data);
    assert_eq!(next.offers().len(), MAX_PENDING_OFFERS);
    assert_eq!(
        next.consider(&cfg, a, "A", "one-more", "L"),
        OfferVerdict::Full
    );
    assert!(!next.newly_full(), "already full: not reported again");
    // An already pending one is still recognized when full.
    assert_eq!(
        next.consider(&cfg, a, "A", "f0", "L"),
        OfferVerdict::AlreadyPending
    );
}
