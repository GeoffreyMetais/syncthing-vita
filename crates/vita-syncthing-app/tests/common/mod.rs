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
use vita_syncthing_app::ui::gfx::Gfx;
use vita_syncthing_app::ui::renderer::draw_frame;

thread_local! {
    /// Software canvas reused by every frame drawn in this test thread.
    static CANVAS: std::cell::RefCell<Gfx> = std::cell::RefCell::new(Gfx::new());
}

/// Draws the frame for `state`, as the main loop does before the user can react to it: the
/// frame declares the interactive elements (`AppState::hit_map`) input is resolved against.
pub fn draw(state: &mut AppState) {
    CANVAS.with(|canvas| draw_frame(&mut canvas.borrow_mut(), state));
}

/// Draws the current frame, then feeds `input` to `InputController`.
pub fn feed(state: &mut AppState, repo: &ConfigRepository, input: InputFrame) -> UserCommand {
    draw(state);
    InputController::handle_input(state, repo, input)
}

/// Feeds one frame with `buttons` newly pressed to `InputController`.
pub fn press(state: &mut AppState, repo: &ConfigRepository, buttons: u32) -> UserCommand {
    feed(
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
    feed(
        state,
        repo,
        InputFrame {
            touch_tap: Some((x, y)),
            any_activity: true,
            ..Default::default()
        },
    )
}
