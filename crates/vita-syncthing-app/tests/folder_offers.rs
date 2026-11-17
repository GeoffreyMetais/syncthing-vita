//! `FolderOfferService`: folders shared by remote devices need the user's consent before they
//! become shares, and declined or deleted shares do not come back on their own.

mod common;

use common::{press, tap_action, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{DeviceRegistry, FolderOfferService, ShareService};
use vita_syncthing_app::domain::{
    ActiveTab, AppState, ModalDialog, UiAction, SETTINGS_ROW_DECLINED,
};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_SQUARE,
};

const PEER: DeviceId = DeviceId([0x42; 32]);

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    DeviceRegistry::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    (dir, repo, state)
}

#[test]
fn test_offered_folder_is_not_shared_without_consent() {
    let (dir, repo, mut state) = setup("offer-pending");

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    assert!(state.shares.is_empty());
    assert!(repo.load_shares().is_empty());
    assert_eq!(state.folder_offers.len(), 1);

    FolderOfferService::prompt_next(&mut state);
    assert_eq!(state.modal, ModalDialog::FolderOffer { offer_idx: 0 });

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_accepting_offer_creates_paused_share_and_opens_folder_picker() {
    let (dir, repo, mut state) = setup("offer-accept");

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::prompt_next(&mut state);
    press(&mut state, &repo, BTN_CROSS);

    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.folder_offers.is_empty());
    assert_eq!(state.shares.len(), 1);
    assert_eq!(state.shares[0].folder_id, "docs");
    assert!(!state.shares[0].enabled);
    assert_eq!(state.shares[0].shared_devices, vec![PEER]);
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    assert_eq!(repo.load_shares(), state.shares);

    // Later ClusterConfigs for an accepted folder do not prompt again
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Documents");
    assert!(state.folder_offers.is_empty());
    assert_eq!(state.shares[0].label, "Documents");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_declined_offer_is_remembered_across_restarts() {
    let (dir, repo, mut state) = setup("offer-decline");

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::prompt_next(&mut state);
    press(&mut state, &repo, BTN_SQUARE);
    assert!(state.shares.is_empty());
    assert!(state.folder_offers.is_empty());

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    assert!(state.folder_offers.is_empty());

    let mut restarted = AppState::new(repo.load_shares(), repo.load_devices(), &dir);
    restarted.ignored_folders = repo.load_ignored_folders();
    FolderOfferService::receive(&mut restarted, &repo, PEER, "docs", "Docs");
    assert!(restarted.folder_offers.is_empty());
    assert!(restarted.shares.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_later_postpones_offer_without_prompting_again() {
    let (dir, repo, mut state) = setup("offer-later");

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::prompt_next(&mut state);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.shares.is_empty());

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::prompt_next(&mut state);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.folder_offers.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_deleted_share_is_declined_and_does_not_come_back() {
    let (dir, repo, mut state) = setup("offer-after-delete");

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::accept(&mut state, &repo, 0);
    assert_eq!(state.shares.len(), 1);

    ShareService::delete_share(&mut state, &repo, 0);
    assert!(state.shares.is_empty());
    assert_eq!(state.ignored_folders.len(), 1);
    assert_eq!(repo.load_ignored_folders(), state.ignored_folders);

    // The peer still lists the folder in its next ClusterConfig: neither a share nor a prompt
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    assert!(state.shares.is_empty());
    assert!(repo.load_shares().is_empty());
    assert!(state.folder_offers.is_empty());

    // Clearing the declined folders lets the peer offer it again
    FolderOfferService::clear_declined(&mut state, &repo);
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    assert_eq!(state.folder_offers.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_settings_clear_declined_lets_peers_offer_again() {
    let (dir, repo, mut state) = setup("offer-clear-declined");

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::decline(&mut state, &repo, 0);
    assert_eq!(state.ignored_folders.len(), 1);
    state.config_dirty = false;

    // Settings: Down past the devices and language cards to the declined folders card, Cross on `Clear Declined`
    state.active_tab = ActiveTab::Settings;
    state.reset_nav_for_tab();
    state.nav.focus = vita_syncthing_app::domain::NavFocus::Content;
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_DOWN);
    press(&mut state, &repo, BTN_DOWN);
    assert!(state.is_settings_row_focused(SETTINGS_ROW_DECLINED));
    press(&mut state, &repo, BTN_CROSS);

    assert!(state.ignored_folders.is_empty());
    assert!(repo.load_ignored_folders().is_empty());
    assert!(
        state.config_dirty,
        "session restarts so peers resend their folders"
    );
    assert_eq!(
        state.theme_mode,
        vita_syncthing_app::domain::ThemeMode::Oled
    );

    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    assert_eq!(state.folder_offers.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_settings_clear_declined_by_touch() {
    let (dir, repo, mut state) = setup("offer-clear-touch");
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::decline(&mut state, &repo, 0);
    state.active_tab = ActiveTab::Settings;

    tap_action(&mut state, &repo, UiAction::ClearDeclinedFolders);
    assert!(state.ignored_folders.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_crafted_folder_label_cannot_inject_a_share() {
    let (dir, repo, mut state) = setup("offer-label-injection");
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::accept(&mut state, &repo, 0);

    // The paired peer later renames the folder with a label forging a second `shares.cfg` line.
    let crafted = format!("Docs|x|0|\nevil|Evil|ux0:tai|1|{}", PEER.to_hex());
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", &crafted);

    let shares = repo.load_shares();
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].folder_id, "docs");
    assert_eq!(shares[0].label, crafted);
    assert!(!shares[0].enabled);
    assert_eq!(shares[0].shared_devices, vec![PEER]);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A folder ID is chosen by the peer: it must never steer the default target of an accepted
/// share outside the `sync` directory.
#[test]
fn test_crafted_folder_id_cannot_escape_the_sync_directory() {
    let (dir, repo, mut state) = setup("offer-traversal");

    FolderOfferService::receive(&mut state, &repo, PEER, "../../outside", "Docs");
    FolderOfferService::prompt_next(&mut state);
    press(&mut state, &repo, BTN_CROSS);

    assert_eq!(state.shares.len(), 1);
    let target = &state.shares[0].target_path;
    assert_eq!(target, &format!("{dir}/sync/outside"));
    assert!(!target.contains(".."));
    assert_eq!(
        repo.default_share_dir("../.."),
        format!("{dir}/sync/folder")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

const OTHER: DeviceId = DeviceId([0x43; 32]);
const THIRD: DeviceId = DeviceId([0x44; 32]);

fn offer_pairs(state: &AppState) -> Vec<(String, DeviceId)> {
    state
        .folder_offers
        .iter()
        .map(|o| (o.folder_id.clone(), o.device_id))
        .collect()
}

#[test]
fn repeated_offers_refresh_only_the_label_of_the_same_peer_and_device() {
    let (dir, repo, mut state) = setup("offers-label-refresh");
    DeviceRegistry::upsert_device(&mut state, &repo, OTHER, "Tablet", "");

    FolderOfferService::receive(&mut state, &repo, PEER, "remote-docs", "Remote Docs");
    FolderOfferService::receive(&mut state, &repo, OTHER, "remote-docs", "Tablet Docs");
    // The same folder offered by another device is a separate offer.
    assert_eq!(state.folder_offers.len(), 2);
    assert_eq!(state.folder_offers[1].label, "Tablet Docs");

    // An empty label never erases the known one.
    FolderOfferService::receive(&mut state, &repo, PEER, "remote-docs", "");
    assert_eq!(state.folder_offers.len(), 2);
    assert_eq!(state.folder_offers[0].label, "Remote Docs");

    // A new label updates the offer of that peer only.
    FolderOfferService::receive(&mut state, &repo, PEER, "remote-docs", "Renamed");
    assert_eq!(state.folder_offers[0].label, "Renamed");
    assert_eq!(state.folder_offers[1].label, "Tablet Docs");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn offer_log_names_the_peer_and_the_folder() {
    let (dir, repo, mut state) = setup("offers-log-name");
    FolderOfferService::receive(&mut state, &repo, PEER, "remote-docs", "Remote Docs");
    assert!(state
        .logs
        .iter()
        .any(|l| l == "MacBook wants to share 'Remote Docs' (remote-docs): waiting for approval"));

    // A peer unknown to the registry is named by its short ID.
    FolderOfferService::receive(&mut state, &repo, THIRD, "other", "");
    let unknown = format!(
        "Device {} wants to share 'other' (other): waiting for approval",
        THIRD.short()
    );
    assert!(state.logs.iter().any(|l| l == &unknown));

    // device_name: known name, the placeholder of a known ID, or the short ID.
    assert_eq!(FolderOfferService::device_name(&state, &PEER), "MacBook");
    let placeholder = DeviceId::parse_user_id(&PEER.short()).unwrap();
    assert_eq!(
        FolderOfferService::device_name(&state, &placeholder),
        "MacBook"
    );
    assert_eq!(
        FolderOfferService::device_name(&state, &THIRD),
        format!("Device {}", THIRD.short())
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn accepting_an_offer_links_its_device_to_an_existing_share_once() {
    let (dir, repo, mut state) = setup("offers-accept-link");
    ShareService::create_share(&mut state, &repo, "docs", "Docs", &format!("{}/docs", dir));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER).unwrap();

    FolderOfferService::receive(&mut state, &repo, OTHER, "docs", "Docs");
    assert_eq!(state.folder_offers.len(), 1);
    FolderOfferService::accept(&mut state, &repo, 0);

    assert_eq!(state.shares.len(), 1, "no second share for the same folder");
    assert_eq!(state.shares[0].shared_devices, vec![PEER, OTHER]);
    assert!(state.folder_offers.is_empty());
    assert_eq!(repo.load_shares()[0].shared_devices, vec![PEER, OTHER]);

    // Accepting an offer whose device is already linked does not duplicate it.
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    FolderOfferService::receive(&mut state, &repo, PEER, "docs", "Docs");
    assert!(
        state.folder_offers.is_empty(),
        "linked peers only refresh the label"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn accepting_an_offer_stops_listing_only_its_own_device_as_unpaired() {
    let (dir, repo, mut state) = setup("offers-accept-unpaired");
    DeviceRegistry::register_discovered_peer(&mut state, OTHER, "", "");
    DeviceRegistry::register_discovered_peer(&mut state, THIRD, "", "");
    FolderOfferService::receive(&mut state, &repo, OTHER, "music", "Music");

    FolderOfferService::accept(&mut state, &repo, 0);
    assert!(!state.is_unpaired_lan_peer(&OTHER));
    assert!(state.is_unpaired_lan_peer(&THIRD));
    assert!(state.shares[0].is_linked_to(&OTHER));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn accepting_a_template_offer_uses_the_preset_folder() {
    let (dir, repo, mut state) = setup("offers-template-target");
    FolderOfferService::receive(&mut state, &repo, PEER, "screenshots", "Screens");
    FolderOfferService::accept(&mut state, &repo, 0);
    assert_eq!(state.shares[0].target_path, "ux0:picture/SCREENSHOT");

    // Other folder IDs get a folder inside the app's sync directory.
    FolderOfferService::receive(&mut state, &repo, PEER, "custom-id", "Custom");
    FolderOfferService::accept(&mut state, &repo, 0);
    assert_eq!(
        state.shares[1].target_path,
        repo.default_share_dir("custom-id")
    );
    assert!(!state.shares[1].enabled);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ignoring_a_folder_drops_only_its_offers_from_the_given_devices() {
    let (dir, repo, mut state) = setup("offers-ignore-scope");
    FolderOfferService::receive(&mut state, &repo, PEER, "a", "");
    FolderOfferService::receive(&mut state, &repo, OTHER, "a", "");
    FolderOfferService::receive(&mut state, &repo, PEER, "b", "");

    FolderOfferService::ignore(&mut state, &repo, "a", &[PEER]);
    assert_eq!(
        offer_pairs(&state),
        vec![("a".to_owned(), OTHER), ("b".to_owned(), PEER)]
    );
    assert_eq!(state.ignored_folders.len(), 1);
    assert!(state.is_folder_ignored("a", &PEER));
    assert!(!state.is_folder_ignored("a", &OTHER));
    assert_eq!(repo.load_ignored_folders(), state.ignored_folders);

    // Ignoring the same pair again does not record it twice.
    FolderOfferService::ignore(&mut state, &repo, "a", &[PEER]);
    assert_eq!(state.ignored_folders.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ignoring_closes_the_offer_modal_only_when_its_offer_is_gone() {
    let (dir, repo, mut state) = setup("offers-ignore-modal");
    FolderOfferService::receive(&mut state, &repo, OTHER, "a", "");
    FolderOfferService::receive(&mut state, &repo, PEER, "b", "");

    // The open offer (index 0) still exists after ignoring `b`: the modal stays.
    state.modal = ModalDialog::FolderOffer { offer_idx: 0 };
    FolderOfferService::ignore(&mut state, &repo, "b", &[PEER]);
    assert_eq!(state.modal, ModalDialog::FolderOffer { offer_idx: 0 });

    // The open offer (index 1) is the one removed: the modal closes.
    FolderOfferService::receive(&mut state, &repo, PEER, "c", "");
    state.modal = ModalDialog::FolderOffer { offer_idx: 1 };
    FolderOfferService::ignore(&mut state, &repo, "c", &[PEER]);
    assert_eq!(state.modal, ModalDialog::None);

    let _ = std::fs::remove_dir_all(&dir);
}
