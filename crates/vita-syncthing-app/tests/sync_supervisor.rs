//! `SyncSupervisor`: life cycle of the sync worker (without a reachable device), and the wait for
//! a background worker of the plugin before the first worker.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use syncthing_core::{DeviceId, DeviceIdentity};
use vita_syncthing_app::application::{DeviceRegistry, ShareService, SyncSupervisor};
use vita_syncthing_app::domain::{AppState, ModalDialog, PendingOffer, UiText};
use vita_syncthing_app::infrastructure::{BackgroundWorkerProbe, ConfigRepository};

fn setup(name: &str) -> (String, ConfigRepository, AppState, SyncSupervisor) {
    let dir = common::temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    let identity = DeviceIdentity::from_seed(&[9; 32]).unwrap();
    (dir, repo, state, SyncSupervisor::new(identity))
}

#[test]
fn test_no_worker_starts_without_a_linked_device() {
    let (dir, repo, mut state, mut sync) = setup("supervisor-idle");

    sync.start_at_launch(&mut state, &repo);
    assert!(!sync.is_running());
    assert_eq!(state.peer_label, UiText::NoPeerLinked);
    assert_eq!(state.status_line, UiText::SelectDeviceToConnect);
    // No worker to ask: the caller starts a sync instead.
    assert!(!sync.request_discovery());

    // Nothing to reconnect to either.
    assert!(!sync.tick(&mut state, &repo));
    assert!(!sync.is_running());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_configuration_change_applies_once_dialogs_are_closed() {
    let (dir, repo, mut state, mut sync) = setup("supervisor-config");
    state.config_dirty = true;
    state.session_connected = true;
    state.pending_reverts.push("saves".to_owned());

    // A dialog is still open: the change waits, and so do the revert requests.
    state.modal = ModalDialog::ConfirmQuit;
    assert!(!sync.tick(&mut state, &repo));
    assert!(state.config_dirty);
    assert_eq!(state.pending_reverts.len(), 1);

    state.modal = ModalDialog::None;
    assert!(sync.tick(&mut state, &repo));
    assert!(!state.config_dirty);
    assert!(!state.session_connected, "the previous sessions are gone");
    assert!(!sync.is_running(), "no linked device: nothing to start");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Background worker probe the test switches on and off.
struct Probe(Arc<AtomicBool>);

impl BackgroundWorkerProbe for Probe {
    fn worker_running(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

const PEER: DeviceId = DeviceId([0x42; 32]);

/// One enabled share linked to `PEER` (unreachable address), and a supervisor whose
/// background probe answers `running`.
fn linked_setup(
    name: &str,
    running: bool,
) -> (
    String,
    ConfigRepository,
    AppState,
    SyncSupervisor,
    Arc<AtomicBool>,
) {
    let dir = common::temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    DeviceRegistry::upsert_device(&mut state, &repo, PEER, "MacBook", "127.0.0.1:9");
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER);
    state.modal = ModalDialog::None;
    state.toast_msg = None;
    // As loaded at launch: nothing left to apply.
    state.config_dirty = false;
    assert!(!state.sync_peers().is_empty());
    let flag = Arc::new(AtomicBool::new(running));
    let identity = DeviceIdentity::from_seed(&[9; 32]).unwrap();
    let sync = SyncSupervisor::with_background_probe(identity, Box::new(Probe(flag.clone())));
    (dir, repo, state, sync, flag)
}

fn log_count(state: &AppState, needle: &str) -> usize {
    state.logs.iter().filter(|l| l.contains(needle)).count()
}

#[test]
fn test_first_worker_waits_for_the_background_worker_to_stop() {
    let (dir, repo, mut state, mut sync, running) = linked_setup("supervisor-handover", true);
    sync.start_at_launch(&mut state, &repo);
    assert!(!sync.is_running());
    assert!(sync.is_waiting_for_background());
    assert_eq!(state.status_line, UiText::WaitingForBackgroundSync);
    assert_eq!(state.peer_label, UiText::BackgroundSyncStopping);
    // Waiting changes nothing on screen and is logged once.
    assert!(!sync.tick(&mut state, &repo));
    assert!(!sync.tick(&mut state, &repo));
    assert!(!sync.is_running());
    assert_eq!(
        log_count(&state, "Waiting for the background sync worker"),
        1
    );
    // A sync asked meanwhile is postponed too.
    sync.start_sync(&mut state, &repo);
    assert!(!sync.is_running());
    assert!(sync.is_waiting_for_background());

    running.store(false, Ordering::SeqCst);
    assert!(sync.tick(&mut state, &repo));
    assert!(sync.is_running());
    assert!(!sync.is_waiting_for_background());
    assert_ne!(state.status_line, UiText::WaitingForBackgroundSync);
    assert_ne!(state.peer_label, UiText::BackgroundSyncStopping);
    assert_eq!(log_count(&state, "Background sync worker stopped after"), 1);
    // Only the first start waits.
    running.store(true, Ordering::SeqCst);
    sync.start_sync(&mut state, &repo);
    assert!(!sync.is_waiting_for_background());
    sync.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_starts_anyway_after_the_bound() {
    let (dir, repo, mut state, mut sync, _running) = linked_setup("supervisor-timeout", true);
    sync.start_at_launch(&mut state, &repo);
    let start = Instant::now();
    assert!(!sync.tick_at(&mut state, &repo, start + Duration::from_secs(29)));
    assert!(!sync.is_running());
    assert!(sync.tick_at(&mut state, &repo, start + Duration::from_secs(31)));
    assert!(sync.is_running());
    assert_eq!(log_count(&state, "still running: starting anyway"), 1);
    sync.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_no_wait_and_no_log_without_a_background_worker() {
    let (dir, repo, mut state, mut sync, _running) = linked_setup("supervisor-no-bg", false);
    sync.start_at_launch(&mut state, &repo);
    assert!(sync.is_running());
    assert!(!sync.is_waiting_for_background());
    assert_eq!(log_count(&state, "Background sync worker"), 0);
    assert_eq!(log_count(&state, "Waiting for the background"), 0);
    sync.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_offers_recorded_by_the_background_worker_are_imported_once_it_stopped() {
    let (dir, repo, mut state, mut sync, running) = linked_setup("supervisor-offers", true);
    sync.start_at_launch(&mut state, &repo);
    // The background worker records an offer before it stops.
    repo.save_pending_offers(&[PendingOffer {
        folder_id: "photos".to_owned(),
        label: "Photos".to_owned(),
        device_id: PEER,
    }]);
    assert!(
        state.folder_offers.is_empty(),
        "not read while it may still write"
    );
    running.store(false, Ordering::SeqCst);
    assert!(sync.tick(&mut state, &repo));
    assert_eq!(state.folder_offers.len(), 1);
    assert_eq!(state.modal, ModalDialog::FolderOffer { offer_idx: 0 });
    sync.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_offers_alone_are_a_screen_change() {
    let (dir, repo, mut state, mut sync, _running) = linked_setup("supervisor-offers-only", false);
    repo.save_pending_offers(&[PendingOffer {
        folder_id: "photos".to_owned(),
        label: "Photos".to_owned(),
        device_id: PEER,
    }]);
    // No worker start postponed: the import alone makes the first tick a change.
    assert!(sync.tick(&mut state, &repo));
    assert_eq!(state.folder_offers.len(), 1);
    sync.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
