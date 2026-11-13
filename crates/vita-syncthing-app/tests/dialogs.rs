//! Simple dialogs (`modal_input/dialogs.rs`): `ConfirmQuit` (buttons and touch targets) and
//! `ConfirmDeleteShare`.

mod common;

use common::{press, rects_of, tap, tap_rect, temp_test_dir};
use vita_syncthing_app::application::{ShareService, UserCommand};
use vita_syncthing_app::domain::{ActiveTab, AppState, ModalDialog, PadKey, UiAction};
use vita_syncthing_app::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE, BTN_START,
};

#[test]
fn test_start_opens_quit_dialog_and_cross_quits() {
    let dir = temp_test_dir("confirm-quit");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // Pressing START opens ConfirmQuit modal instead of quitting immediately
    assert_eq!(press(&mut state, &repo, BTN_START), UserCommand::None);
    assert_eq!(state.modal, ModalDialog::ConfirmQuit);

    // Pressing CROSS inside ConfirmQuit returns UserCommand::QuitApp
    assert_eq!(press(&mut state, &repo, BTN_CROSS), UserCommand::QuitApp);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_confirm_quit_touch_targets() {
    let dir = temp_test_dir("input-ctl-quit-touch");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);

    // The `Quit App` button and its hint both quit.
    state.modal = ModalDialog::ConfirmQuit;
    let quit = rects_of(&mut state, UiAction::Press(PadKey::Cross));
    assert_eq!(quit.len(), 2, "the dialog button and the hint");
    for rect in quit {
        state.modal = ModalDialog::ConfirmQuit;
        assert_eq!(tap_rect(&mut state, &repo, rect), UserCommand::QuitApp);
    }

    // Touching the dialog body or outside it cancels instead of quitting.
    for (x, y) in [(480, 200), (10, 10)] {
        state.modal = ModalDialog::ConfirmQuit;
        assert_eq!(tap(&mut state, &repo, x, y), UserCommand::None);
        assert_eq!(state.modal, ModalDialog::None);
    }
}

#[test]
fn test_square_on_overview_asks_confirmation_before_deleting() {
    let dir = temp_test_dir("delete-share-input");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::create_share(&mut state, &repo, "a", "Share A", &format!("{}/a", dir));
    state.active_tab = ActiveTab::Dashboard;

    // Square opens the confirmation; Circle cancels without deleting
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(
        state.modal,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 }
    );
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.modal, ModalDialog::None);
    assert_eq!(state.shares.len(), 1);

    // Square then Cross deletes the share
    press(&mut state, &repo, BTN_SQUARE);
    press(&mut state, &repo, BTN_CROSS);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.shares.is_empty());

    // Square with no share does nothing
    press(&mut state, &repo, BTN_SQUARE);
    assert_eq!(state.modal, ModalDialog::None);

    let _ = std::fs::remove_dir_all(&dir);
}
