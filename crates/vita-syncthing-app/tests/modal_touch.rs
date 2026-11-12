//! Modal dialogs: touch is resolved from the elements each modal declares while drawing.
//! Touching a button is the same as pressing the pad button it shows; rows and keypad keys
//! are activated by index; no coordinate is known outside the views.

mod common;

use common::{draw, press, rect_of, rects_of, tap, tap_action, tap_rect, temp_test_dir};
use syncthing_core::DeviceId;
use vita_syncthing_app::application::{FolderOfferService, ShareService, UserCommand};
use vita_syncthing_app::domain::{
    AppState, Language, LanguagePreference, ModalDialog, PadKey, Rect, UiAction, ADD_DEVICE_KEYPAD,
    NEW_SHARE_TEMPLATES,
};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_TRIANGLE};
use vita_syncthing_app::ui::gfx::{GFX_H, GFX_W};
use vita_syncthing_app::ui::theme::HINT_Y;

const PEER: DeviceId = DeviceId([0x42; 32]);
const OTHER: DeviceId = DeviceId([0x43; 32]);

/// State with one share and two known devices, `PEER` linked to the share.
fn setup(name: &str) -> (String, ConfigRepository, AppState) {
    let dir = temp_test_dir(name);
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), &dir);
    ShareService::upsert_device(&mut state, &repo, PEER, "MacBook", "192.168.1.10:22000");
    ShareService::upsert_device(&mut state, &repo, OTHER, "NAS", "192.168.1.11:22000");
    ShareService::create_share(&mut state, &repo, "saves", "Saves", &format!("{dir}/saves"));
    ShareService::toggle_device_for_share(&mut state, &repo, 0, PEER);
    state.modal = ModalDialog::None;
    (dir, repo, state)
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[test]
fn test_every_modal_element_is_on_screen_and_hints_do_not_overlap() {
    let (dir, repo, mut state) = setup("modal-geometry");
    FolderOfferService::receive(&mut state, &repo, OTHER, "docs", "Docs");
    let modals = [
        ModalDialog::CreateShare { selected_idx: 0 },
        ModalDialog::ManageDevices {
            share_idx: 0,
            selected_dev_idx: 0,
        },
        ModalDialog::AddDeviceById {
            return_share_idx: None,
            keypad_idx: 0,
        },
        ModalDialog::ScanDeviceQr {
            return_share_idx: None,
        },
        ModalDialog::ShowDeviceId,
        ModalDialog::ConfirmDeleteShare { share_idx: 0 },
        ModalDialog::ConfirmQuit,
        ModalDialog::FolderOffer { offer_idx: 0 },
    ];
    for language in [
        Language::English,
        Language::French,
        Language::Spanish,
        Language::German,
        Language::Italian,
    ] {
        state.set_language_preference(LanguagePreference::Fixed(language));
        for keypad in [false, true] {
            state.fallback_keypad_visible = keypad;
            for modal in modals {
                state.modal = modal;
                draw(&mut state);
                let hotspots = state.hit_map.hotspots().to_vec();
                assert!(!hotspots.is_empty());
                for h in &hotspots {
                    let r = h.rect;
                    assert!(
                        r.x >= 0 && r.y >= 0 && r.x + r.w <= GFX_W && r.y + r.h <= GFX_H,
                        "{:?} is off screen in {modal:?} ({language:?}): {r:?}",
                        h.action
                    );
                }
                // Buttons of the dialog never cover each other (backdrop and body come first).
                let buttons: Vec<&Rect> = hotspots
                    .iter()
                    .skip(2)
                    .filter(|h| matches!(h.action, UiAction::Press(_)) && h.rect.y < HINT_Y)
                    .map(|h| &h.rect)
                    .collect();
                for (i, a) in buttons.iter().enumerate() {
                    for b in &buttons[i + 1..] {
                        assert!(
                            !overlap(a, b),
                            "buttons overlap in {modal:?} ({language:?}): {a:?} / {b:?}"
                        );
                    }
                }
                // Hints are declared last, after the full-screen backdrop.
                let hints: Vec<&Rect> = hotspots
                    .iter()
                    .skip(1)
                    .map(|h| &h.rect)
                    .filter(|r| r.y >= HINT_Y)
                    .collect();
                for (i, a) in hints.iter().enumerate() {
                    for b in &hints[i + 1..] {
                        assert!(
                            !overlap(a, b),
                            "hints overlap in {modal:?} ({language:?}): {a:?} / {b:?}"
                        );
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_page_elements_are_not_interactive_behind_a_modal() {
    let (dir, _repo, mut state) = setup("modal-blocks-page");
    draw(&mut state);
    assert!(state
        .hit_map
        .hotspots()
        .iter()
        .any(|h| matches!(h.action, UiAction::SwitchTab(_))));
    state.modal = ModalDialog::ConfirmQuit;
    draw(&mut state);
    assert!(!state
        .hit_map
        .hotspots()
        .iter()
        .any(|h| matches!(h.action, UiAction::SwitchTab(_) | UiAction::StartSync)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_create_share_template_row_and_outside_touch() {
    let (dir, repo, mut state) = setup("modal-create-share");
    // Touching outside the dialog closes it; touching its body does nothing.
    state.modal = ModalDialog::CreateShare { selected_idx: 0 };
    let body = rects_of(&mut state, UiAction::Inert)[0];
    tap(
        &mut state,
        &repo,
        (body.x + 20) as usize,
        (body.y + 20) as usize,
    );
    assert!(matches!(state.modal, ModalDialog::CreateShare { .. }));
    tap(&mut state, &repo, 5, 5);
    assert_eq!(state.modal, ModalDialog::None);

    // Touching a template picks it: the share is created and its devices are asked for.
    state.modal = ModalDialog::CreateShare { selected_idx: 0 };
    let music = NEW_SHARE_TEMPLATES
        .iter()
        .position(|&(id, _, _)| id == "vita-music")
        .unwrap();
    tap_action(&mut state, &repo, UiAction::ModalRow(music));
    assert!(matches!(state.modal, ModalDialog::ManageDevices { .. }));
    assert!(state.shares.iter().any(|s| s.folder_id == "vita-music"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_manage_devices_rows_and_buttons() {
    let (dir, repo, mut state) = setup("modal-manage-devices");
    let manage = ModalDialog::ManageDevices {
        share_idx: 0,
        selected_dev_idx: 0,
    };
    let other_row = state
        .devices
        .iter()
        .position(|d| d.device_id == OTHER)
        .unwrap();

    // Touching a device row links it, touching it again unlinks it.
    state.modal = manage;
    tap_action(&mut state, &repo, UiAction::ModalRow(other_row));
    assert!(state.shares[0].shared_devices.contains(&OTHER.to_hex()));
    tap_action(&mut state, &repo, UiAction::ModalRow(other_row));
    assert!(!state.shares[0].shared_devices.contains(&OTHER.to_hex()));

    // The `Pause` button (shown with SELECT) pauses the share.
    assert!(state.shares[0].enabled);
    let pause = rects_of(&mut state, UiAction::Press(PadKey::Select))[0];
    tap_rect(&mut state, &repo, pause);
    assert!(!state.shares[0].enabled);

    // The `Done` button closes the dialog.
    let done = rects_of(&mut state, UiAction::Press(PadKey::Circle));
    tap_rect(&mut state, &repo, done[1]);
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_add_device_text_box_keypad_and_buttons() {
    let (dir, repo, mut state) = setup("modal-add-device");
    state.modal = ModalDialog::AddDeviceById {
        return_share_idx: None,
        keypad_idx: 0,
    };
    // Touching the text box opens the system keyboard.
    let text_box = rect_of(&mut state, UiAction::OpenKeyboard);
    assert_eq!(
        tap(
            &mut state,
            &repo,
            (text_box.x + 20) as usize,
            (text_box.y + 10) as usize
        ),
        UserCommand::OpenSystemKeyboard
    );

    // The keypad keys are only declared while the virtual keyboard is shown.
    draw(&mut state);
    assert!(!state
        .hit_map
        .hotspots()
        .iter()
        .any(|h| matches!(h.action, UiAction::ModalRow(_))));
    press(&mut state, &repo, BTN_TRIANGLE);
    assert!(state.fallback_keypad_visible);

    // Touching keys types them; the `Backspace` button removes the last one.
    for label in ["M", "F", "Z"] {
        let key = ADD_DEVICE_KEYPAD.iter().position(|&k| k == label).unwrap();
        tap_action(&mut state, &repo, UiAction::ModalRow(key));
    }
    assert_eq!(state.device_id_input, "MFZ");
    let backspace = rects_of(&mut state, UiAction::Press(PadKey::Square))[0];
    tap_rect(&mut state, &repo, backspace);
    assert_eq!(state.device_id_input, "MF");

    // Touching outside the dialog cancels.
    tap(&mut state, &repo, 5, 5);
    assert_eq!(state.modal, ModalDialog::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_folder_offer_buttons_and_inert_outside() {
    let (dir, repo, mut state) = setup("modal-folder-offer");
    FolderOfferService::receive(&mut state, &repo, OTHER, "docs", "Docs");
    FolderOfferService::prompt_next(&mut state);
    assert_eq!(state.modal, ModalDialog::FolderOffer { offer_idx: 0 });

    // A consent dialog is not dismissed by touching around it.
    tap(&mut state, &repo, 5, 5);
    assert_eq!(state.modal, ModalDialog::FolderOffer { offer_idx: 0 });

    // The `Decline` button (shown with Square) declines the offer for good.
    let decline = rects_of(&mut state, UiAction::Press(PadKey::Square))[0];
    tap_rect(&mut state, &repo, decline);
    assert_eq!(state.modal, ModalDialog::None);
    assert!(state.is_folder_ignored("docs", &OTHER));
    assert!(state.shares.iter().all(|s| s.folder_id != "docs"));
    let _ = std::fs::remove_dir_all(&dir);
}
