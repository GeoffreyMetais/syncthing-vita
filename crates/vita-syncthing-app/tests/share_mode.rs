//! Share access mode: read & write or read only (peers never modify the PS Vita's files).

mod common;

use common::{draw, press, rects_of, tap_rect, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{ShareModeService, ShareService};
use vita_syncthing_app::domain::{
    AppState, Language, LanguagePreference, ModalDialog, PadKey, Rect, ShareMode, UiAction, UiText,
};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_L};
use vita_syncthing_app::ui::gfx::{GFX_H, GFX_W};
use vita_syncthing_app::ui::theme::HINT_Y;

const PEER: DeviceId = DeviceId([0x42; 32]);

/// State with one share `saves` linked to `PEER`, its `ManageDevices` modal open.
fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER);
    state.modal = ModalDialog::ManageDevices {
        share_idx: 0,
        selected_dev_idx: 0,
    };
    state.config_dirty = false;
    (dir, repo, state)
}

#[test]
fn test_share_mode_codes() {
    assert_eq!(ShareMode::default(), ShareMode::ReadWrite);
    for mode in [ShareMode::ReadWrite, ShareMode::ReadOnly] {
        assert_eq!(ShareMode::from_code(mode.code()), mode);
        assert_ne!(mode.toggled(), mode);
        assert_eq!(mode.toggled().toggled(), mode);
    }
    assert_eq!(ShareMode::from_code("garbage"), ShareMode::ReadWrite);
}

#[test]
fn test_new_share_is_read_write_and_mode_is_saved() {
    let (dir, repo, mut state) = setup("share-mode-saved");
    assert_eq!(state.shares[0].mode, ShareMode::ReadWrite);

    ShareModeService::toggle(&mut state, &repo, 0);
    assert_eq!(state.shares[0].mode, ShareMode::ReadOnly);
    // The worker restarts with the new mode, which survives a restart of the app.
    assert!(state.config_dirty);
    assert!(matches!(
        &state.toast_msg,
        Some((UiText::ShareReadOnly { id }, _)) if id == "saves"
    ));
    assert_eq!(repo.load_shares()[0].mode, ShareMode::ReadOnly);

    ShareModeService::toggle(&mut state, &repo, 0);
    assert_eq!(repo.load_shares()[0].mode, ShareMode::ReadWrite);
    // A stale index is ignored.
    ShareModeService::toggle(&mut state, &repo, 7);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_legacy_shares_cfg_without_mode_is_read_write() {
    let dir = temp_test_dir("share-mode-legacy");
    let line = format!("saves|Saves|ux0:data/saves|1|{}\n", PEER.to_hex());
    std::fs::write(format!("{dir}/shares.cfg"), line).unwrap();
    let shares = ConfigRepository::new(&dir).load_shares();
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].mode, ShareMode::ReadWrite);
    assert_eq!(shares[0].shared_devices, vec![PEER.to_hex()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_manage_devices_l_button_switches_mode() {
    let (dir, repo, mut state) = setup("share-mode-modal");

    // `L` switches the mode without closing the dialog.
    draw(&mut state);
    press(&mut state, &repo, BTN_L);
    assert_eq!(state.shares[0].mode, ShareMode::ReadOnly);
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));

    // Touching the mode button is the same as pressing `L`.
    let button = rects_of(&mut state, UiAction::Press(PadKey::L))[0];
    tap_rect(&mut state, &repo, button);
    assert_eq!(state.shares[0].mode, ShareMode::ReadWrite);
    let _ = std::fs::remove_dir_all(&dir);
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[test]
fn test_mode_button_fits_in_every_language() {
    let (dir, repo, mut state) = setup("share-mode-geometry");
    for language in Language::ALL {
        state.set_language_preference(LanguagePreference::Fixed(language));
        for _ in 0..2 {
            ShareModeService::toggle(&mut state, &repo, 0);
            draw(&mut state);
            let buttons: Vec<Rect> = state
                .hit_map
                .hotspots()
                .iter()
                .skip(2)
                .filter(|h| matches!(h.action, UiAction::Press(_)) && h.rect.y < HINT_Y)
                .map(|h| h.rect)
                .collect();
            let mode = rects_of(&mut state, UiAction::Press(PadKey::L))[0];
            assert!(mode.x >= 0 && mode.x + mode.w <= GFX_W && mode.y + mode.h <= GFX_H);
            for b in buttons.iter().filter(|b| **b != mode) {
                assert!(!overlap(&mode, b), "{language:?}: {mode:?} / {b:?}");
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
