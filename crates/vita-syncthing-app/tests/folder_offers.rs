//! `FolderOfferService`: folders shared by remote devices need the user's consent before they
//! become shares, and declined or deleted shares do not come back on their own.

mod common;

use common::{press, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{FolderOfferService, InputController, ShareService};
use vita_syncthing_app::domain::{ActiveTab, AppState, ModalDialog, SETTINGS_ROW_DECLINED};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_SQUARE,
};
use vita_syncthing_app::ui::theme::{SETTINGS_DECLINED_B, SETTINGS_DECLINED_Y};

const PEER: DeviceId = DeviceId([0x42; 32]);

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
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
    assert_eq!(state.shares[0].shared_devices, vec![PEER.to_hex()]);
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

    // Settings: Down past the language card to the declined folders card, Cross on `Clear Declined`
    state.active_tab = ActiveTab::Settings;
    state.reset_nav_for_tab();
    state.nav.focus = vita_syncthing_app::domain::NavFocus::Content;
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

    InputController::handle_input(
        &mut state,
        &repo,
        InputFrame {
            touch_tap: Some((
                860,
                (SETTINGS_DECLINED_Y + SETTINGS_DECLINED_B) as usize / 2,
            )),
            any_activity: true,
            ..Default::default()
        },
    );
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
    assert_eq!(shares[0].shared_devices, vec![PEER.to_hex()]);

    let _ = std::fs::remove_dir_all(&dir);
}
