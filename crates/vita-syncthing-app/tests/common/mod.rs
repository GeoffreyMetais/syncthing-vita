//! Shared helpers for `vita-syncthing-app` integration tests (each test binary uses a subset).
#![allow(dead_code)]

/// Creates an empty, unique directory under `/tmp` for one test.
pub fn temp_test_dir(suffix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = format!("/tmp/vita-syncthing-test-{}-{}", suffix, nanos);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

use vita_syncthing_app::application::{InputController, UserCommand};
use vita_syncthing_app::domain::AppState;
use vita_syncthing_app::infrastructure::{ConfigRepository, InputFrame};

/// Feeds one frame with `buttons` newly pressed to `InputController`.
pub fn press(state: &mut AppState, repo: &ConfigRepository, buttons: u32) -> UserCommand {
    InputController::handle_input(
        state,
        repo,
        InputFrame {
            pressed: buttons,
            any_activity: true,
            ..Default::default()
        },
    )
}

/// Feeds one frame with a touch tap at screen coordinates (`x`, `y`) to `InputController`.
pub fn tap(state: &mut AppState, repo: &ConfigRepository, x: usize, y: usize) -> UserCommand {
    InputController::handle_input(
        state,
        repo,
        InputFrame {
            touch_tap: Some((x, y)),
            any_activity: true,
            ..Default::default()
        },
    )
}
