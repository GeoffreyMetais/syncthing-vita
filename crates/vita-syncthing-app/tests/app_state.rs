//! `AppState` queries over devices, sessions, shares and folder offers (zero I/O).

use syncthing_core::DeviceId;
use vita_syncthing_app::domain::{
    AppState, FolderOffer, IgnoredFolder, KnownDevice, ModalDialog, ShareConfigEntry, ShareMode,
    SyncActivity, UiText,
};

const A: DeviceId = DeviceId([0x41; 32]);
const B: DeviceId = DeviceId([0x42; 32]);
const C: DeviceId = DeviceId([0x43; 32]);

fn short_of(full: DeviceId) -> DeviceId {
    DeviceId::parse_user_id(&full.short()).expect("short ID parses")
}

fn device(id: DeviceId, name: &str) -> KnownDevice {
    KnownDevice {
        device_id: id,
        name: name.to_owned(),
        address: "tcp://192.0.2.1:22000".to_owned(),
    }
}

fn share(folder_id: &str, enabled: bool, linked: &[DeviceId]) -> ShareConfigEntry {
    ShareConfigEntry {
        folder_id: folder_id.to_owned(),
        label: format!("label {folder_id}"),
        target_path: format!("ux0:data/{folder_id}"),
        enabled,
        mode: ShareMode::ReadWrite,
        shared_devices: linked.to_vec(),
    }
}

fn state(shares: Vec<ShareConfigEntry>, devices: Vec<KnownDevice>) -> AppState {
    AppState::new(shares, devices, "ux0:")
}

#[test]
fn test_new_has_no_session_and_no_peers() {
    let s = state(Vec::new(), Vec::new());
    assert!(!s.session_connected);
    assert!(!s.connected_peers.is_active());
    assert!(!s.initial_sync.is_active());
    assert!(s.auto_reconnect_enabled);
    assert!(s.approved_devices().is_empty());
    assert!(!s.has_configured_peers());
    assert_eq!(s.picker.current_dir, "ux0:");
}

#[test]
fn test_reset_sessions_clears_session_and_both_peer_sets() {
    let mut s = state(Vec::new(), Vec::new());
    s.session_connected = true;
    s.connected_peers.begin(A);
    s.initial_sync.begin(B);
    s.reset_sessions();
    assert!(!s.session_connected);
    assert!(!s.connected_peers.is_active());
    assert!(!s.initial_sync.is_active());
}

#[test]
fn test_unpaired_lan_peer_matches_full_id_of_a_short_placeholder() {
    let mut s = state(Vec::new(), Vec::new());
    s.unpaired_lan_peers = vec![short_of(A)];
    assert!(s.is_unpaired_lan_peer(&A), "full ID of the placeholder");
    assert!(!s.is_unpaired_lan_peer(&B));

    s.unpaired_lan_peers = vec![B];
    assert!(s.is_unpaired_lan_peer(&B), "exact match");
    assert!(!s.is_unpaired_lan_peer(&A));
}

#[test]
fn test_approved_devices_excludes_unlinked_unpaired_peers_only() {
    let mut s = state(
        vec![share("docs", true, &[B])],
        vec![device(A, "a"), device(B, "b"), device(C, "c")],
    );
    s.unpaired_lan_peers = vec![A, B];
    let names: Vec<String> = s.approved_devices().into_iter().map(|d| d.name).collect();
    // A is unpaired and not linked: excluded. B is unpaired but linked: kept. C is not unpaired.
    assert_eq!(names, vec!["b".to_owned(), "c".to_owned()]);
    assert!(s.has_configured_peers());
}

#[test]
fn test_has_configured_peers_is_false_when_only_unpaired_peers_are_known() {
    let mut s = state(Vec::new(), vec![device(A, "a")]);
    assert!(s.has_configured_peers());
    s.unpaired_lan_peers = vec![A];
    assert!(!s.has_configured_peers());
}

#[test]
fn test_sync_peers_without_shares_is_every_approved_device() {
    let mut s = state(Vec::new(), vec![device(A, "a"), device(B, "b")]);
    s.unpaired_lan_peers = vec![B];
    let peers = s.sync_peers();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].device_id, A);
    assert_eq!(peers, s.approved_devices());
}

#[test]
fn test_sync_peers_need_an_enabled_share_linked_to_them() {
    let s = state(
        vec![share("on", true, &[A]), share("off", false, &[B])],
        vec![device(A, "a"), device(B, "b"), device(C, "c")],
    );
    let ids: Vec<DeviceId> = s.sync_peers().into_iter().map(|d| d.device_id).collect();
    // A has an enabled linked share; B's only share is paused; C is linked to nothing.
    assert_eq!(ids, vec![A]);
}

#[test]
fn test_sync_peers_accept_a_placeholder_link_for_the_full_id() {
    let s = state(
        vec![share("on", true, &[short_of(A)])],
        vec![device(A, "a")],
    );
    assert_eq!(s.sync_peers().len(), 1);
}

#[test]
fn test_is_sync_ready_needs_a_session_and_no_download() {
    let mut s = state(Vec::new(), Vec::new());
    assert!(!s.is_sync_ready());
    s.session_connected = true;
    assert!(s.is_sync_ready());
    s.sync_in_progress = true;
    assert!(!s.is_sync_ready());
    s.session_connected = false;
    s.sync_in_progress = false;
    assert!(!s.is_sync_ready());
}

#[test]
fn test_sync_activity_precedence_download_then_upload_then_ready() {
    let mut s = state(Vec::new(), Vec::new());
    assert_eq!(s.sync_activity(), SyncActivity::Idle);

    s.session_connected = true;
    assert_eq!(s.sync_activity(), SyncActivity::Ready);

    s.uploading = true;
    assert_eq!(s.sync_activity(), SyncActivity::Uploading);

    s.sync_in_progress = true;
    assert_eq!(
        s.sync_activity(),
        SyncActivity::Downloading,
        "download wins over upload"
    );

    s.session_connected = false;
    s.uploading = false;
    assert_eq!(s.sync_activity(), SyncActivity::Downloading);

    s.sync_in_progress = false;
    assert_eq!(s.sync_activity(), SyncActivity::Idle);
}

#[test]
fn test_sync_activity_upload_without_session_is_still_uploading() {
    let mut s = state(Vec::new(), Vec::new());
    s.uploading = true;
    assert_eq!(s.sync_activity(), SyncActivity::Uploading);
}

#[test]
fn test_queue_folder_offer_keeps_each_id_once_in_order() {
    let mut s = state(Vec::new(), Vec::new());
    s.queue_folder_offer("photos");
    s.queue_folder_offer("docs");
    s.queue_folder_offer("photos");
    assert_eq!(s.pending_folder_offers, vec!["photos", "docs"]);
}

#[test]
fn test_is_folder_ignored_needs_the_same_folder_and_device() {
    let mut s = state(Vec::new(), Vec::new());
    s.ignored_folders = vec![IgnoredFolder {
        folder_id: "f".to_owned(),
        device_id: A,
    }];
    assert!(s.is_folder_ignored("f", &A));
    assert!(!s.is_folder_ignored("g", &A));
    assert!(!s.is_folder_ignored("f", &B));
}

#[test]
fn test_is_folder_ignored_matches_a_placeholder_for_the_full_id() {
    let mut s = state(Vec::new(), Vec::new());
    s.ignored_folders = vec![IgnoredFolder {
        folder_id: "f".to_owned(),
        device_id: short_of(A),
    }];
    assert!(s.is_folder_ignored("f", &A));
    assert!(!s.is_folder_ignored("f", &B));
}

fn offer(id: &str, snoozed: bool) -> FolderOffer {
    FolderOffer {
        folder_id: id.to_owned(),
        label: format!("label {id}"),
        device_id: A,
        snoozed,
    }
}

#[test]
fn test_next_folder_offer_to_prompt_skips_postponed_offers() {
    let mut s = state(Vec::new(), Vec::new());
    assert_eq!(s.next_folder_offer_to_prompt(), None);
    s.folder_offers = vec![offer("a", true), offer("b", false), offer("c", false)];
    assert_eq!(s.next_folder_offer_to_prompt(), Some(1));
    s.folder_offers = vec![offer("a", true), offer("b", true)];
    assert_eq!(s.next_folder_offer_to_prompt(), None);
}

#[test]
fn test_share_creation_is_incomplete_until_a_device_is_linked() {
    let mut s = state(vec![share("new", true, &[])], Vec::new());
    assert!(!s.is_share_creation_incomplete(0), "not being created");
    s.pending_share_creation = Some("new".to_owned());
    assert!(s.is_share_creation_incomplete(0));
    assert!(!s.is_share_creation_incomplete(1), "no such share");

    s.shares[0].shared_devices.push(A);
    assert!(!s.is_share_creation_incomplete(0), "a device is linked");

    s.shares[0].shared_devices.clear();
    s.pending_share_creation = Some("other".to_owned());
    assert!(
        !s.is_share_creation_incomplete(0),
        "another share is being created"
    );
}

#[test]
fn test_toggle_theme_switches_and_announces_the_new_theme() {
    use vita_syncthing_app::domain::ThemeMode;
    let mut s = state(Vec::new(), Vec::new());
    assert_eq!(s.theme_mode, ThemeMode::Oled);
    s.toggle_theme();
    assert_eq!(s.theme_mode, ThemeMode::Dark);
    assert_eq!(s.toast_msg, Some((UiText::ThemeDark, 110)));
    s.toggle_theme();
    assert_eq!(s.theme_mode, ThemeMode::Oled);
    assert_eq!(s.toast_msg, Some((UiText::ThemeOled, 110)));
}

#[test]
fn test_is_scanning_qr_only_for_the_scanner_modal() {
    let mut s = state(Vec::new(), Vec::new());
    assert!(!s.is_scanning_qr());
    s.modal = ModalDialog::ScanDeviceQr {
        return_share_idx: None,
    };
    assert!(s.is_scanning_qr());
    s.modal = ModalDialog::ShowDeviceId;
    assert!(!s.is_scanning_qr());
}

#[test]
fn test_sanitize_legacy_unpaired_peers_unlinks_legacy_names_once() {
    let mut s = state(
        vec![share("docs", true, &[A, B, C])],
        vec![
            device(A, "LAN Peer"),
            device(B, "Syncthing Peer"),
            device(C, "Syncthing Peer (x)"),
        ],
    );
    assert!(s.sanitize_legacy_unpaired_peers());
    // "Syncthing Peer (x)" is not one of the legacy names: it stays linked.
    assert_eq!(s.unpaired_lan_peers, vec![A, B]);
    assert_eq!(s.shares[0].shared_devices, vec![C]);
    assert!(!s.sanitize_legacy_unpaired_peers(), "nothing left to clean");
}

#[test]
fn test_sanitize_legacy_unpaired_peers_recognises_prefixed_names() {
    let mut s = state(
        Vec::new(),
        vec![
            device(A, "Syncthing (old)"),
            device(B, "LAN Peer (1.2.3.4)"),
            device(C, "Mine"),
        ],
    );
    assert!(s.sanitize_legacy_unpaired_peers());
    assert_eq!(s.unpaired_lan_peers, vec![A, B]);
}

#[test]
fn test_sanitize_legacy_unpaired_peers_without_legacy_names_changes_nothing() {
    let mut s = state(Vec::new(), vec![device(A, "Laptop")]);
    assert!(!s.sanitize_legacy_unpaired_peers());
    assert!(s.unpaired_lan_peers.is_empty());
}
