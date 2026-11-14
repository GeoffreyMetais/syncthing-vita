//! `InputGestures`: held `Up` / `Down` repeat, touch tap on release, touch drag.

use vita_syncthing_app::application::InputGestures;
use vita_syncthing_app::domain::defaults::{
    KEY_REPEAT_DELAY, KEY_REPEAT_INTERVAL, TOUCH_DRAG_THRESHOLD,
};
use vita_syncthing_app::infrastructure::{InputFrame, BTN_DOWN, BTN_LEFT, BTN_UP};

/// Applies the gestures to one frame and returns it.
fn step(gestures: &mut InputGestures, mut frame: InputFrame) -> InputFrame {
    gestures.apply(&mut frame);
    frame
}

fn held(buttons: u32) -> InputFrame {
    InputFrame {
        held: buttons,
        ..Default::default()
    }
}

fn pressed(buttons: u32) -> InputFrame {
    InputFrame {
        pressed: buttons,
        held: buttons,
        ..Default::default()
    }
}

fn touch(pos: (usize, usize), down: bool) -> InputFrame {
    InputFrame {
        touch_pos: Some(pos),
        touch_down: down.then_some(pos),
        ..Default::default()
    }
}

#[test]
fn test_held_down_repeats_after_the_delay() {
    let mut g = InputGestures::new();
    assert_eq!(step(&mut g, pressed(BTN_DOWN)).pressed, BTN_DOWN);
    let mut repeats = Vec::new();
    for vblank in 1..=KEY_REPEAT_DELAY + 2 * KEY_REPEAT_INTERVAL {
        if step(&mut g, held(BTN_DOWN)).pressed & BTN_DOWN != 0 {
            repeats.push(vblank);
        }
    }
    assert_eq!(
        repeats,
        vec![
            KEY_REPEAT_DELAY,
            KEY_REPEAT_DELAY + KEY_REPEAT_INTERVAL,
            KEY_REPEAT_DELAY + 2 * KEY_REPEAT_INTERVAL
        ]
    );
}

#[test]
fn test_repeat_stops_on_release_and_restarts_with_a_new_press() {
    let mut g = InputGestures::new();
    step(&mut g, pressed(BTN_UP));
    for _ in 1..KEY_REPEAT_DELAY {
        assert_eq!(step(&mut g, held(BTN_UP)).pressed, 0);
    }
    // Released just before the first repeat: nothing more.
    assert_eq!(step(&mut g, held(0)).pressed, 0);
    assert_eq!(step(&mut g, held(BTN_UP)).pressed, 0);
    // Switching direction restarts the delay.
    step(&mut g, pressed(BTN_DOWN));
    for _ in 1..KEY_REPEAT_DELAY {
        assert_eq!(step(&mut g, held(BTN_DOWN)).pressed, 0);
    }
    assert_eq!(step(&mut g, held(BTN_DOWN)).pressed, BTN_DOWN);
}

#[test]
fn test_other_buttons_never_repeat() {
    let mut g = InputGestures::new();
    step(&mut g, pressed(BTN_LEFT));
    for _ in 0..3 * KEY_REPEAT_DELAY {
        assert_eq!(step(&mut g, held(BTN_LEFT)).pressed, 0);
    }
}

#[test]
fn test_touch_is_a_tap_on_release_at_its_start() {
    let mut g = InputGestures::new();
    let down = step(&mut g, touch((300, 200), true));
    assert_eq!(down.touch_tap, None);
    // Jitter within the threshold is still a tap.
    let moved = (300, 200 + TOUCH_DRAG_THRESHOLD as usize);
    assert_eq!(step(&mut g, touch(moved, false)).touch_drag, None);
    let up = step(&mut g, InputFrame::default());
    assert_eq!(up.touch_tap, Some((300, 200)));
    // Nothing after the release.
    assert_eq!(step(&mut g, InputFrame::default()).touch_tap, None);
}

#[test]
fn test_touch_moving_beyond_the_threshold_is_a_drag_not_a_tap() {
    let mut g = InputGestures::new();
    step(&mut g, touch((300, 400), true));
    let first = step(&mut g, touch((302, 380), false));
    let drag = first.touch_drag.expect("drag recognized");
    assert_eq!(
        (drag.start, drag.total_dy, drag.started),
        ((300, 400), -20, true)
    );
    assert!(first.changes_view());
    let next = step(&mut g, touch((302, 300), false)).touch_drag.unwrap();
    assert_eq!((next.total_dy, next.started), (-100, false));
    assert_eq!(step(&mut g, InputFrame::default()).touch_tap, None);
}

#[test]
fn test_injected_tap_passes_through() {
    let mut g = InputGestures::new();
    let frame = step(
        &mut g,
        InputFrame {
            touch_tap: Some((10, 20)),
            ..Default::default()
        },
    );
    assert_eq!(frame.touch_tap, Some((10, 20)));
}
