//! `InputController` global routing: an active OLED screensaver swallows input and wakes up on a
//! button press or touch contact.

mod common;

use common::{press, temp_test_dir};
use vita_syncthing_app::application::InputController;
use vita_syncthing_app::domain::AppState;
use vita_syncthing_app::infrastructure::{ConfigRepository, InputFrame, BTN_CIRCLE, BTN_SELECT};

#[test]
fn test_screensaver_wakes_on_press_or_touch_only() {
    let dir = temp_test_dir("saver-wake");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");

    press(&mut state, &repo, BTN_SELECT);
    assert!(state.oled_saver_active);
    // Activity without a press or touch contact (scripted `wait` frames) keeps it running.
    let idle = InputFrame {
        any_activity: true,
        ..Default::default()
    };
    InputController::handle_input(&mut state, &repo, idle);
    assert!(state.oled_saver_active);
    // Resting a finger on the screen wakes it.
    let touch = InputFrame {
        touch_pos: Some((100, 100)),
        any_activity: true,
        ..Default::default()
    };
    InputController::handle_input(&mut state, &repo, touch);
    assert!(!state.oled_saver_active);
    // So does any button.
    press(&mut state, &repo, BTN_SELECT);
    press(&mut state, &repo, BTN_CIRCLE);
    assert!(!state.oled_saver_active);
}
