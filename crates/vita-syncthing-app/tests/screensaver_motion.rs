//! `ScreensaverMotion`: bouncing of the OLED screensaver widget within the screen margins.
//! Speeds and positions below are exact in binary floating point, so edges are asserted exactly.

use vita_syncthing_app::domain::ScreensaverMotion;

/// Screen of the PS Vita, and a widget of 100 x 50 pixels.
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 544.0;
const BOX_W: f32 = 100.0;
const BOX_H: f32 = 50.0;
/// Edge margin of `screensaver.rs` (12 px): the widget stays within `12..=max`.
const MARGIN: f32 = 12.0;
/// Largest top-left position on the right / bottom: 960 - 100 - 12 and 544 - 50 - 12.
const MAX_X: f32 = 848.0;
const MAX_Y: f32 = 482.0;

fn motion(x: f32, y: f32, vx: f32, vy: f32) -> ScreensaverMotion {
    ScreensaverMotion { x, y, vx, vy }
}

fn step(m: &mut ScreensaverMotion, frames: u32) {
    m.step(SCREEN_W, SCREEN_H, BOX_W, BOX_H, frames);
}

#[test]
fn test_default_starts_near_the_centre_and_moves_diagonally() {
    let m = ScreensaverMotion::default();
    assert_eq!((m.x, m.y), (180.0, 160.0));
    assert_eq!((m.vx, m.vy), (0.65, 0.45));
}

#[test]
fn test_step_moves_by_speed_per_frame() {
    let mut m = motion(100.0, 200.0, 2.0, -1.0);
    step(&mut m, 1);
    assert_eq!((m.x, m.y), (102.0, 199.0));
    assert_eq!((m.vx, m.vy), (2.0, -1.0), "no bounce away from the edges");
}

#[test]
fn test_zero_frames_do_not_move_the_widget() {
    let mut m = motion(300.0, 300.0, 3.0, 3.0);
    step(&mut m, 0);
    assert_eq!(m, motion(300.0, 300.0, 3.0, 3.0));
}

#[test]
fn test_frames_are_applied_one_by_one() {
    let mut a = motion(100.0, 100.0, 1.5, 0.5);
    let mut b = a;
    step(&mut a, 7);
    for _ in 0..7 {
        step(&mut b, 1);
    }
    assert_eq!(a, b);
    assert_eq!((a.x, a.y), (110.5, 103.5));
}

#[test]
fn test_right_edge_is_inclusive_of_the_limit() {
    // Exactly on the limit: no bounce.
    let mut m = motion(847.0, 100.0, 1.0, 0.0);
    step(&mut m, 1);
    assert_eq!(m.x, MAX_X);
    assert_eq!(m.vx, 1.0);

    // Beyond the limit: clamped and reflected.
    let mut m = motion(847.5, 100.0, 1.0, 0.0);
    step(&mut m, 1);
    assert_eq!(m.x, MAX_X);
    assert_eq!(m.vx, -1.0);
}

#[test]
fn test_left_edge_is_inclusive_of_the_margin() {
    // Exactly on the margin: no bounce.
    let mut m = motion(13.0, 100.0, -1.0, 0.0);
    step(&mut m, 1);
    assert_eq!(m.x, MARGIN);
    assert_eq!(m.vx, -1.0);

    // Beyond the margin: clamped and reflected.
    let mut m = motion(12.5, 100.0, -1.0, 0.0);
    step(&mut m, 1);
    assert_eq!(m.x, MARGIN);
    assert_eq!(m.vx, 1.0);
}

#[test]
fn test_top_and_bottom_edges_bounce_independently() {
    let mut m = motion(300.0, 481.5, 0.0, 1.0);
    step(&mut m, 1);
    assert_eq!(m.y, MAX_Y);
    assert_eq!(m.vy, -1.0);
    assert_eq!((m.x, m.vx), (300.0, 0.0));

    let mut m = motion(300.0, 12.5, 0.0, -1.0);
    step(&mut m, 1);
    assert_eq!(m.y, MARGIN);
    assert_eq!(m.vy, 1.0);
}

#[test]
fn test_bounce_reverses_the_motion_on_following_frames() {
    // 845 -> 847 -> 849 (bounces to 848, speed -2) -> 846.
    let mut m = motion(845.0, 100.0, 2.0, 0.0);
    step(&mut m, 2);
    assert_eq!((m.x, m.vx), (848.0, -2.0));
    step(&mut m, 1);
    assert_eq!((m.x, m.vx), (846.0, -2.0));
}

#[test]
fn test_widget_stays_within_bounds_over_a_long_run() {
    let mut m = ScreensaverMotion::default();
    for _ in 0..5000 {
        step(&mut m, 3);
        assert!(m.x >= MARGIN && m.x <= MAX_X, "x out of bounds: {}", m.x);
        assert!(m.y >= MARGIN && m.y <= MAX_Y, "y out of bounds: {}", m.y);
    }
}

#[test]
fn test_margins_depend_on_the_widget_size() {
    // A larger widget reaches its right edge sooner: the limit is screen - box - margin.
    let mut m = motion(700.0, 100.0, 1.0, 0.0);
    m.step(SCREEN_W, SCREEN_H, 200.0, BOX_H, 1);
    assert_eq!(m.x, 701.0, "700 + 1 is still within 960 - 200 - 12 = 748");

    let mut m = motion(747.5, 100.0, 1.0, 0.0);
    m.step(SCREEN_W, SCREEN_H, 200.0, BOX_H, 1);
    assert_eq!(m.x, 748.0);
    assert_eq!(m.vx, -1.0);
}
