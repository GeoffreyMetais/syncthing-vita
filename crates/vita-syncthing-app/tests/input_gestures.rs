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

#[test]
fn test_touch_drag_threshold_is_exclusive_in_both_directions() {
    // A finger exactly at the threshold is still a tap; one more pixel is a drag.
    let threshold = TOUCH_DRAG_THRESHOLD as usize;
    for (dx, dy) in [(threshold, 0), (0, threshold), (0, 0)] {
        let mut g = InputGestures::new();
        step(&mut g, touch((300, 300), true));
        let moved = (300 + dx, 300 + dy);
        assert_eq!(step(&mut g, touch(moved, false)).touch_drag, None);
        assert_eq!(
            step(&mut g, InputFrame::default()).touch_tap,
            Some((300, 300))
        );
    }
    for (x, y) in [(300 + threshold + 1, 300), (300 - threshold - 1, 300), (300, 300 - threshold - 1)] {
        let mut g = InputGestures::new();
        step(&mut g, touch((300, 300), true));
        assert!(step(&mut g, touch((x, y), false)).touch_drag.is_some());
        assert_eq!(step(&mut g, InputFrame::default()).touch_tap, None);
    }
}

#[test]
fn test_drag_continues_when_the_finger_returns_inside_the_threshold() {
    let mut g = InputGestures::new();
    step(&mut g, touch((300, 300), true));
    assert!(step(&mut g, touch((300, 200), false)).touch_drag.is_some());
    // Back at the start: the touch is still a drag, so it never becomes a tap.
    let back = step(&mut g, touch((300, 300), false)).touch_drag.expect("still dragging");
    assert_eq!((back.total_dy, back.started), (0, false));
    assert_eq!(step(&mut g, InputFrame::default()).touch_tap, None);
}

#[test]
fn test_a_new_touch_down_resets_the_previous_drag() {
    let mut g = InputGestures::new();
    step(&mut g, touch((300, 300), true));
    step(&mut g, touch((300, 100), false));
    step(&mut g, InputFrame::default());
    // Second touch: starts a fresh gesture, small movement is a tap again.
    step(&mut g, touch((50, 50), true));
    let small = step(&mut g, touch((52, 50), false));
    assert_eq!(small.touch_drag, None);
    assert_eq!(step(&mut g, InputFrame::default()).touch_tap, Some((50, 50)));
}

#[test]
fn test_drag_reports_its_start_and_the_started_flag_only_once() {
    let mut g = InputGestures::new();
    step(&mut g, touch((200, 200), true));
    let first = step(&mut g, touch((200, 180), false)).touch_drag.unwrap();
    assert!(first.started);
    assert_eq!(first.start, (200, 200));
    let second = step(&mut g, touch((200, 170), false)).touch_drag.unwrap();
    assert!(!second.started);
    assert_eq!((second.start, second.total_dy), ((200, 200), -30));
}

#[test]
fn test_release_keeps_an_injected_tap_and_nothing_else_is_emitted() {
    let mut g = InputGestures::new();
    step(&mut g, touch((10, 10), true));
    let released = step(
        &mut g,
        InputFrame {
            touch_tap: Some((99, 99)),
            ..Default::default()
        },
    );
    assert_eq!(released.touch_tap, Some((99, 99)));
    assert_eq!(released.touch_drag, None);
}

#[test]
fn test_repeat_follows_the_held_direction_only() {
    let mut g = InputGestures::new();
    step(&mut g, pressed(BTN_UP));
    // Holding Down instead of Up (without a new press) ends the repeat.
    for _ in 0..KEY_REPEAT_DELAY + KEY_REPEAT_INTERVAL {
        assert_eq!(step(&mut g, held(BTN_DOWN)).pressed & BTN_UP, 0);
    }
    assert_eq!(step(&mut g, held(BTN_DOWN)).pressed, 0);
}
