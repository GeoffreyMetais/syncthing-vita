//! `SyncSupervisor`: life cycle of the sync worker, without any linked device (no network).

mod common;

use syncthing_core::DeviceIdentity;
use vita_syncthing_app::application::SyncSupervisor;
use vita_syncthing_app::domain::{AppState, ModalDialog, UiText};
use vita_syncthing_app::infrastructure::ConfigRepository;

fn setup(name: &str) -> (String, ConfigRepository, AppState, SyncSupervisor) {
    let dir = common::temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    let identity = DeviceIdentity::from_seed(&[9; 32]).unwrap();
    (dir, repo, state, SyncSupervisor::new(identity))
}

#[test]
fn test_worker_starts_at_launch_even_without_a_linked_device() {
    let (dir, repo, mut state, mut sync) = setup("supervisor-idle");

    sync.start_at_launch(&mut state, &repo);
    assert!(
        sync.is_running(),
        "inbound reachability worker starts even with 0 paired devices"
    );
    assert_eq!(state.peer_label, UiText::NoPeerLinked);
    assert_eq!(state.status_line, UiText::SelectDeviceToConnect);
    assert_eq!(
        state.progress_pct,
        Some(0),
        "idle listener must not animate indeterminate progress"
    );
    assert!(sync.request_discovery());

    sync.shutdown();
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
    assert!(
        sync.is_running(),
        "replacement worker starts to keep inbound reachability up"
    );

    sync.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
