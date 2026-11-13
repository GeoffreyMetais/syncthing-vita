//! Wi-Fi prompt: an offline console asks the user to turn Wi-Fi on, the prompt closes by itself
//! once connected and the sync worker is restarted.

mod common;

use std::time::{Duration, Instant};

use common::{press, rects_of, tap, tap_rect, temp_test_dir};
use vita_syncthing_app::application::{WifiChange, WifiPrompt};
use vita_syncthing_app::domain::defaults::WIFI_PROMPT_DELAY;
use vita_syncthing_app::domain::{AppState, ModalDialog, PadKey, UiAction, UiText, WifiLink};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_CIRCLE, BTN_START};

fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let state = AppState::new(Vec::new(), Vec::new(), &dir);
    (dir, repo, state)
}

/// Reports `Disconnected` long enough for the prompt to open.
fn go_offline(wifi: &mut WifiPrompt, state: &mut AppState, t0: Instant) {
    wifi.update(state, WifiLink::Disconnected, t0);
    wifi.update(state, WifiLink::Disconnected, t0 + WIFI_PROMPT_DELAY);
}

#[test]
fn test_prompt_opens_after_delay_and_closes_once_connected() {
    let (dir, _repo, mut state) = setup("wifi-prompt-open-close");
    let mut wifi = WifiPrompt::new();
    let t0 = Instant::now();

    assert_eq!(
        wifi.update(&mut state, WifiLink::Disconnected, t0),
        WifiChange::Updated
    );
    assert!(state.wifi.is_offline());
    // A short drop does not prompt.
    wifi.update(
        &mut state,
        WifiLink::Disconnected,
        t0 + Duration::from_millis(500),
    );
    assert_eq!(state.modal, ModalDialog::None);

    wifi.update(&mut state, WifiLink::Disconnected, t0 + WIFI_PROMPT_DELAY);
    assert_eq!(state.modal, ModalDialog::WifiOff);

    // Associating keeps the prompt open; the connection closes it and restarts the worker.
    wifi.update(&mut state, WifiLink::Connecting, t0 + WIFI_PROMPT_DELAY * 2);
    assert_eq!(state.modal, ModalDialog::WifiOff);
    assert_eq!(
        wifi.update(&mut state, WifiLink::Connected, t0 + WIFI_PROMPT_DELAY * 3),
        WifiChange::Restored
    );
    assert_eq!(state.modal, ModalDialog::None);
    assert!(matches!(state.toast_msg, Some((UiText::WifiConnected, _))));

    // Staying connected changes nothing.
    assert_eq!(
        wifi.update(&mut state, WifiLink::Connected, t0 + WIFI_PROMPT_DELAY * 4),
        WifiChange::None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_connected_at_launch_or_unknown_never_prompts() {
    let (dir, _repo, mut state) = setup("wifi-prompt-never");
    let mut wifi = WifiPrompt::new();
    let t0 = Instant::now();
    assert_eq!(
        wifi.update(&mut state, WifiLink::Connected, t0),
        WifiChange::Updated
    );
    for link in [WifiLink::Unknown, WifiLink::Connecting] {
        wifi.update(&mut state, link, t0 + WIFI_PROMPT_DELAY * 5);
        assert_eq!(state.modal, ModalDialog::None);
    }
    // A brief disconnection interrupted by `Connecting` does not prompt either.
    wifi.update(&mut state, WifiLink::Disconnected, t0);
    wifi.update(&mut state, WifiLink::Connecting, t0 + WIFI_PROMPT_DELAY / 2);
    wifi.update(&mut state, WifiLink::Disconnected, t0 + WIFI_PROMPT_DELAY);
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_prompt_waits_for_other_modals() {
    let (dir, _repo, mut state) = setup("wifi-prompt-other-modal");
    let mut wifi = WifiPrompt::new();
    let t0 = Instant::now();
    state.modal = ModalDialog::ShowDeviceId;
    go_offline(&mut wifi, &mut state, t0);
    assert_eq!(state.modal, ModalDialog::ShowDeviceId);
    state.modal = ModalDialog::None;
    wifi.update(
        &mut state,
        WifiLink::Disconnected,
        t0 + WIFI_PROMPT_DELAY * 2,
    );
    assert_eq!(state.modal, ModalDialog::WifiOff);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_continue_offline_until_next_connection() {
    let (dir, repo, mut state) = setup("wifi-prompt-dismiss");
    let mut wifi = WifiPrompt::new();
    let t0 = Instant::now();
    go_offline(&mut wifi, &mut state, t0);
    assert_eq!(state.modal, ModalDialog::WifiOff);

    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    wifi.update(
        &mut state,
        WifiLink::Disconnected,
        t0 + WIFI_PROMPT_DELAY * 10,
    );
    assert_eq!(state.modal, ModalDialog::None);

    // Connected, then offline again: the prompt comes back.
    wifi.update(&mut state, WifiLink::Connected, t0 + WIFI_PROMPT_DELAY * 11);
    go_offline(&mut wifi, &mut state, t0 + WIFI_PROMPT_DELAY * 12);
    assert_eq!(state.modal, ModalDialog::WifiOff);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_prompt_touch_and_quit() {
    let (dir, repo, mut state) = setup("wifi-prompt-touch");
    let mut wifi = WifiPrompt::new();
    let t0 = Instant::now();
    go_offline(&mut wifi, &mut state, t0);

    // A stray touch keeps the instructions on screen.
    tap(&mut state, &repo, 5, 5);
    assert_eq!(state.modal, ModalDialog::WifiOff);

    // `START` asks to quit; cancelling brings the prompt back while still offline.
    press(&mut state, &repo, BTN_START);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);
    press(&mut state, &repo, BTN_CIRCLE);
    wifi.update(
        &mut state,
        WifiLink::Disconnected,
        t0 + WIFI_PROMPT_DELAY * 2,
    );
    assert_eq!(state.modal, ModalDialog::WifiOff);

    // Touching `Continue offline` dismisses it.
    let button = rects_of(&mut state, UiAction::Press(PadKey::Circle))[0];
    tap_rect(&mut state, &repo, button);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.wifi.prompt_dismissed);
    let _ = std::fs::remove_dir_all(&dir);
}
