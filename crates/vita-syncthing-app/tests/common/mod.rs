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
use vita_syncthing_app::domain::{AppState, Rect, UiAction};
use vita_syncthing_app::infrastructure::{ConfigRepository, InputFrame, TouchDrag};
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

/// Rectangles of the elements that trigger `action` in the frame drawn for `state`, in
/// drawing order (a card first, then the buttons on top of it).
pub fn rects_of(state: &mut AppState, action: UiAction) -> Vec<Rect> {
    draw(state);
    state
        .hit_map
        .hotspots()
        .iter()
        .filter(|h| h.action == action)
        .map(|h| h.rect)
        .collect()
}

/// Rectangle of the first element that triggers `action`; panics if it is not on screen.
pub fn rect_of(state: &mut AppState, action: UiAction) -> Rect {
    *rects_of(state, action)
        .first()
        .unwrap_or_else(|| panic!("{action:?} is not on screen"))
}

/// Taps the centre of `rect`.
pub fn tap_rect(state: &mut AppState, repo: &ConfigRepository, rect: Rect) -> UserCommand {
    tap(
        state,
        repo,
        (rect.x + rect.w / 2) as usize,
        (rect.y + rect.h / 2) as usize,
    )
}

/// Taps the element of the current frame that triggers `action` (the topmost one when
/// several do, e.g. a button drawn on a card with the same action).
pub fn tap_action(state: &mut AppState, repo: &ConfigRepository, action: UiAction) -> UserCommand {
    let rect = *rects_of(state, action)
        .last()
        .unwrap_or_else(|| panic!("{action:?} is not on screen"));
    tap_rect(state, repo, rect)
}

/// Drags a finger vertically by `dy` pixels from the centre of the first scroll area of the
/// current frame declared for `target`, over `steps` frames, then lifts it.
pub fn drag_over(
    state: &mut AppState,
    repo: &ConfigRepository,
    target: vita_syncthing_app::domain::ScrollTarget,
    dy: i32,
    steps: i32,
) {
    draw(state);
    let area = *state
        .hit_map
        .scroll_areas()
        .iter()
        .find(|a| a.target == target)
        .unwrap_or_else(|| panic!("no {target:?} scroll area on screen"));
    let start = (
        (area.rect.x + area.rect.w / 2) as usize,
        (area.rect.y + area.rect.h / 2) as usize,
    );
    for i in 1..=steps.max(1) {
        let total_dy = dy * i / steps.max(1);
        let pos = (start.0, (start.1 as i32 + total_dy).clamp(0, 543) as usize);
        feed(
            state,
            repo,
            InputFrame {
                touch_drag: Some(TouchDrag {
                    start,
                    total_dy,
                    started: i == 1,
                }),
                touch_pos: Some(pos),
                any_activity: true,
                ..Default::default()
            },
        );
    }
}
