//! `pending_offers.cfg`: folder offers recorded by the plugin's background worker while the app
//! was closed. Exact file format, hostile input, limits, import into the consent flow at
//! startup (same filtering as live offers) and removal once answered or no longer relevant.

mod common;

use common::{press, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, FolderOfferService, ShareService};
use vita_syncthing_app::domain::defaults::MAX_PENDING_OFFERS;
use vita_syncthing_app::domain::{AppState, ModalDialog, PendingOffer};
use vita_syncthing_app::infrastructure::pending_offers::PENDING_OFFERS_FILE;
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE};

const PEER: DeviceId = DeviceId([0x42; 32]);
const OTHER: DeviceId = DeviceId([0x17; 32]);
const STRANGER: DeviceId = DeviceId([0x99; 32]);

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    DeviceRegistry::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    DeviceRegistry::upsert_device(&mut state, &repo, OTHER, "Desktop", "192.168.1.11:22000");
    (dir, repo, state)
}

fn offer(folder_id: &str, label: &str, device_id: DeviceId) -> PendingOffer {
    PendingOffer {
        folder_id: folder_id.to_owned(),
        label: label.to_owned(),
        device_id,
    }
}

fn file_path(dir: &str) -> String {
    format!("{dir}/{PENDING_OFFERS_FILE}")
}

fn write_file(dir: &str, text: &str) {
    std::fs::write(file_path(dir), text).unwrap();
}

fn file_text(dir: &str) -> Option<String> {
    std::fs::read_to_string(file_path(dir)).ok()
}

#[test]
fn test_exact_file_format_with_escaped_fields() {
    let (dir, repo, _state) = setup("pending-format");
    assert_eq!(PENDING_OFFERS_FILE, "pending_offers.cfg");
    assert!(repo.load_pending_offers().is_empty());
    repo.save_pending_offers(&[
        offer("docs", "Docs", PEER),
        offer("a|b", "50% off\nline|2", OTHER),
    ]);
    assert_eq!(
        file_text(&dir).unwrap(),
        format!(
            "docs|Docs|{}\na%7Cb|50%25 off%0Aline%7C2|{}\n",
            PEER.to_hex(),
            OTHER.to_hex()
        )
    );
    assert_eq!(
        repo.load_pending_offers(),
        [
            offer("docs", "Docs", PEER),
            offer("a|b", "50% off\nline|2", OTHER)
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_invalid_and_duplicate_lines_are_dropped() {
    let (dir, repo, _state) = setup("pending-invalid");
    let peer = PEER.to_hex();
    write_file(
        &dir,
        &format!(
            "docs|Docs|{peer}\n\
             docs|Docs again|{peer}\n\
             |No ID|{peer}\n\
             music|Music|not-hex\n\
             two|fields\n\
             four|fields|{peer}|extra\n\
             \n\
             pics|Pictures| {peer} \r\n"
        ),
    );
    assert_eq!(
        repo.load_pending_offers(),
        [offer("docs", "Docs", PEER), offer("pics", "Pictures", PEER)]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_at_most_32_offers_are_read() {
    let (dir, repo, _state) = setup("pending-limit");
    assert_eq!(MAX_PENDING_OFFERS, 32);
    let text: String = (0..=MAX_PENDING_OFFERS)
        .map(|i| format!("f{i}|F{i}|{}\n", PEER.to_hex()))
        .collect();
    write_file(&dir, &text);
    let offers = repo.load_pending_offers();
    assert_eq!(offers.len(), MAX_PENDING_OFFERS);
    assert_eq!(offers[MAX_PENDING_OFFERS - 1].folder_id, "f31");
    // Exactly the limit: all kept.
    let text: String = (0..MAX_PENDING_OFFERS)
        .map(|i| format!("g{i}|G|{}\n", PEER.to_hex()))
        .collect();
    write_file(&dir, &text);
    assert_eq!(repo.load_pending_offers().len(), MAX_PENDING_OFFERS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_remove_one_offer_and_the_file_once_empty() {
    let (dir, repo, _state) = setup("pending-remove");
    repo.save_pending_offers(&[offer("docs", "Docs", PEER), offer("docs", "Docs", OTHER)]);
    // Another folder or device: nothing removed.
    repo.remove_pending_offer("music", &PEER);
    repo.remove_pending_offer("docs", &STRANGER);
    assert_eq!(repo.load_pending_offers().len(), 2);
    repo.remove_pending_offer("docs", &PEER);
    assert_eq!(repo.load_pending_offers(), [offer("docs", "Docs", OTHER)]);
    repo.remove_pending_offer("docs", &OTHER);
    assert!(file_text(&dir).is_none(), "an empty list removes the file");
    // Nothing to remove from a missing file.
    repo.remove_pending_offer("docs", &OTHER);
    assert!(file_text(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_import_opens_the_consent_dialog_and_later_keeps_the_offer() {
    let (dir, repo, mut state) = setup("pending-import");
    repo.save_pending_offers(&[offer("docs", "Docs", PEER)]);
    assert_eq!(FolderOfferService::import_pending(&mut state, &repo), 1);
    assert!(state.shares.is_empty(), "never shared without consent");
    assert_eq!(state.folder_offers.len(), 1);
    assert_eq!(state.folder_offers[0].label, "Docs");
    FolderOfferService::prompt_next(&mut state);
    assert_eq!(state.modal, ModalDialog::FolderOffer { offer_idx: 0 });
    // `Later`: kept in the file for the next start.
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(repo.load_pending_offers(), [offer("docs", "Docs", PEER)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_accept_removes_the_offer_from_the_file() {
    let (dir, repo, mut state) = setup("pending-accept");
    repo.save_pending_offers(&[offer("docs", "Docs", PEER), offer("music", "Music", PEER)]);
    FolderOfferService::import_pending(&mut state, &repo);
    FolderOfferService::prompt_next(&mut state);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.shares.len(), 1);
    assert_eq!(repo.load_pending_offers(), [offer("music", "Music", PEER)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_decline_removes_the_offer_and_ignores_it() {
    let (dir, repo, mut state) = setup("pending-decline");
    repo.save_pending_offers(&[offer("docs", "Docs", PEER)]);
    FolderOfferService::import_pending(&mut state, &repo);
    FolderOfferService::prompt_next(&mut state);
    press(&mut state, &repo, BTN_SQUARE);
    assert!(state.is_folder_ignored("docs", &PEER));
    assert!(file_text(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_offer_answered_live_is_removed_from_the_file() {
    let (dir, repo, mut state) = setup("pending-live");
    // Recorded in the background, then received again live before the import (or answered
    // after arriving live in a later session): the answer clears the file entry too.
    repo.save_pending_offers(&[offer("docs", "Docs", PEER)]);
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::accept(&mut state, &repo, 0);
    assert!(file_text(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_irrelevant_and_unknown_offers_are_dropped_from_the_file() {
    let (dir, repo, mut state) = setup("pending-filter");
    // Already shared with PEER, declined for OTHER, from an unknown device, duplicate of a
    // live offer.
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER);
    state.modal = ModalDialog::None;
    FolderOfferService::ignore(&mut state, &repo, "music", &[OTHER]);
    FolderOfferService::receive(&mut state, &repo, PEER, "live", "Live");
    repo.save_pending_offers(&[
        offer("saves", "Saves", PEER),
        offer("music", "Music", OTHER),
        offer("docs", "Docs", STRANGER),
        offer("live", "Live", PEER),
        offer("saves", "Saves", OTHER),
    ]);
    assert_eq!(FolderOfferService::import_pending(&mut state, &repo), 2);
    assert_eq!(
        repo.load_pending_offers(),
        [offer("live", "Live", PEER), offer("saves", "Saves", OTHER)]
    );
    // The live offer is not duplicated; the share not linked to OTHER is offered.
    assert_eq!(state.folder_offers.len(), 2);
    assert!(state
        .folder_offers
        .iter()
        .any(|o| o.folder_id == "saves" && o.device_id == OTHER));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_ignoring_a_folder_removes_its_pending_offers() {
    let (dir, repo, mut state) = setup("pending-ignore");
    repo.save_pending_offers(&[offer("docs", "Docs", PEER), offer("docs", "Docs", OTHER)]);
    FolderOfferService::import_pending(&mut state, &repo);
    // Deleting a share declines it for its devices.
    FolderOfferService::ignore(&mut state, &repo, "docs", &[PEER]);
    assert_eq!(repo.load_pending_offers(), [offer("docs", "Docs", OTHER)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_hostile_label_cannot_inject_an_offer_or_a_share() {
    let (dir, repo, mut state) = setup("pending-hostile");
    // A label carrying a field separator and a line break, written by the worker escaped.
    let hostile = format!("x|y|{}\nevil|Evil|{}", PEER.to_hex(), PEER.to_hex());
    repo.save_pending_offers(&[offer("docs", &hostile, PEER)]);
    assert_eq!(file_text(&dir).unwrap().lines().count(), 1);
    assert_eq!(FolderOfferService::import_pending(&mut state, &repo), 1);
    assert_eq!(state.folder_offers.len(), 1);
    assert_eq!(state.folder_offers[0].folder_id, "docs");
    assert_eq!(state.folder_offers[0].label, hostile);
    assert!(state.shares.is_empty());
    // Unescaped separators written by a broken writer only make the line invalid.
    write_file(&dir, &format!("docs|a|b|{}\n", PEER.to_hex()));
    assert!(repo.load_pending_offers().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_import_without_file_does_nothing() {
    let (dir, repo, mut state) = setup("pending-none");
    assert_eq!(FolderOfferService::import_pending(&mut state, &repo), 0);
    assert!(state.folder_offers.is_empty());
    assert!(file_text(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
