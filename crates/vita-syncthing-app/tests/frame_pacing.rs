//! Dynamic frame rate: frames are only rendered when the screen changes.

use vita_syncthing_app::application::frame_pacing::{
    ANIMATION_INTERVAL, BACKGROUND_INTERVAL, IDLE_REFRESH_INTERVAL,
};
use vita_syncthing_app::application::{FramePacer, FrameSignals};
use vita_syncthing_app::domain::{AppState, UiText};

const USER: FrameSignals = FrameSignals {
    user_change: true,
    background_change: false,
    animating: false,
};
const BACKGROUND: FrameSignals = FrameSignals {
    user_change: false,
    background_change: true,
    animating: false,
};
const ANIMATION: FrameSignals = FrameSignals {
    user_change: false,
    background_change: false,
    animating: true,
};
const IDLE: FrameSignals = FrameSignals {
    user_change: false,
    background_change: false,
    animating: false,
};

/// Ticks `vblanks` times with `signals` and returns how many frames were rendered.
fn count_frames(pacer: &mut FramePacer, signals: FrameSignals, vblanks: u32) -> u32 {
    (0..vblanks)
        .filter(|_| pacer.tick(signals).is_some())
        .count() as u32
}

fn settled_pacer() -> FramePacer {
    let mut pacer = FramePacer::new();
    assert!(pacer.tick(IDLE).is_some(), "the first VBlank renders");
    pacer
}

#[test]
fn test_static_screen_renders_one_frame_per_second() {
    let mut pacer = settled_pacer();
    assert_eq!(count_frames(&mut pacer, IDLE, IDLE_REFRESH_INTERVAL - 1), 0);
    assert_eq!(pacer.tick(IDLE), Some(IDLE_REFRESH_INTERVAL));
    assert_eq!(count_frames(&mut pacer, IDLE, 600), 10);
}

#[test]
fn test_user_input_renders_on_the_next_vblank() {
    let mut pacer = settled_pacer();
    assert_eq!(pacer.tick(USER), Some(1));
    assert_eq!(count_frames(&mut pacer, USER, 60), 60);
    assert_eq!(pacer.tick(IDLE), None);
}

#[test]
fn test_animation_runs_at_thirty_fps() {
    let mut pacer = settled_pacer();
    assert_eq!(
        count_frames(&mut pacer, ANIMATION, 60),
        60 / ANIMATION_INTERVAL
    );
    assert_eq!(pacer.tick(ANIMATION), None);
    assert_eq!(pacer.tick(ANIMATION), Some(ANIMATION_INTERVAL));
}

#[test]
fn test_background_changes_are_coalesced() {
    let mut pacer = settled_pacer();
    // Worker messages on every VBlank (fast transfer): at most 10 FPS.
    assert_eq!(
        count_frames(&mut pacer, BACKGROUND, 60),
        60 / BACKGROUND_INTERVAL
    );
    // A single change is not lost: it is rendered once the interval has elapsed.
    let mut pacer = settled_pacer();
    assert_eq!(pacer.tick(BACKGROUND), None);
    let rendered = (1..BACKGROUND_INTERVAL)
        .filter_map(|_| pacer.tick(IDLE))
        .next();
    assert_eq!(rendered, Some(BACKGROUND_INTERVAL));
    assert_eq!(pacer.tick(IDLE), None);
}

#[test]
fn test_toast_expiry_is_counted_in_vblanks() {
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");
    state.show_toast(UiText::SyncingWithPeers);
    let ticks = (0..1000).take_while(|_| !state.tick_toast()).count();
    assert_eq!(ticks + 1, 110);
    assert!(state.toast_msg.is_none());
    assert!(!state.tick_toast(), "no toast: nothing to redraw");
}

#[test]
fn test_screensaver_speed_does_not_depend_on_frame_rate() {
    let new_state = || AppState::new(Vec::new(), Vec::new(), "ux0:");
    let mut every_vblank = new_state();
    let mut every_other = new_state();
    for _ in 0..30 {
        every_vblank.step_screensaver(960.0, 544.0, 410.0, 132.0, 1);
        every_vblank.step_screensaver(960.0, 544.0, 410.0, 132.0, 1);
        every_other.step_screensaver(960.0, 544.0, 410.0, 132.0, 2);
    }
    assert_eq!(every_vblank.saver_x, every_other.saver_x);
    assert_eq!(every_vblank.saver_y, every_other.saver_y);
}
