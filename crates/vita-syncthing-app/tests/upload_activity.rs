//! `UploadActivity`: the "Uploading" state ends once no block was served for `UPLOAD_DISPLAY_GRACE`.

use std::time::{Duration, Instant};

use vita_syncthing_app::application::upload_activity::UPLOAD_DISPLAY_GRACE;
use vita_syncthing_app::application::UploadActivity;
use vita_syncthing_app::domain::AppState;

fn state() -> AppState {
    AppState::new(Vec::new(), Vec::new(), "ux0:")
}

#[test]
fn nothing_to_end_when_not_uploading() {
    let mut activity = UploadActivity::new();
    let mut state = state();
    assert!(!activity.update(&mut state, Instant::now()));
    assert!(!state.uploading);
}

#[test]
fn uploading_without_any_served_block_ends_at_once() {
    let mut activity = UploadActivity::new();
    let mut state = state();
    state.uploading = true;
    assert!(activity.update(&mut state, Instant::now()));
    assert!(!state.uploading);
    assert!(
        !activity.update(&mut state, Instant::now()),
        "nothing changes twice"
    );
}

#[test]
fn upload_state_lasts_until_exactly_one_grace_period_after_the_last_block() {
    let mut activity = UploadActivity::new();
    let mut state = state();
    let t0 = Instant::now();

    state.uploading = true;
    state.blocks_uploaded = 1;
    assert!(!activity.update(&mut state, t0), "a block was just served");
    assert!(state.uploading);

    // The grace period is inclusive: still uploading at its end.
    assert!(!activity.update(&mut state, t0 + UPLOAD_DISPLAY_GRACE));
    assert!(state.uploading);

    // One millisecond later, the state ends and the screen must be redrawn.
    assert!(activity.update(
        &mut state,
        t0 + UPLOAD_DISPLAY_GRACE + Duration::from_millis(1)
    ));
    assert!(!state.uploading);
}

#[test]
fn new_blocks_restart_the_grace_period_and_quiet_calls_do_not() {
    let mut activity = UploadActivity::new();
    let mut state = state();
    let t0 = Instant::now();
    state.uploading = true;
    state.blocks_uploaded = 1;
    activity.update(&mut state, t0);

    // Quiet calls do not move the reference time.
    let almost = t0 + UPLOAD_DISPLAY_GRACE - Duration::from_millis(10);
    assert!(!activity.update(&mut state, almost));

    // A block served just before the deadline restarts the countdown.
    state.blocks_uploaded = 2;
    state.uploading = true;
    assert!(!activity.update(&mut state, almost));
    let after_old_deadline = t0 + UPLOAD_DISPLAY_GRACE + Duration::from_millis(50);
    assert!(!activity.update(&mut state, after_old_deadline));
    assert!(state.uploading);

    let new_deadline = almost + UPLOAD_DISPLAY_GRACE + Duration::from_millis(1);
    assert!(activity.update(&mut state, new_deadline));
    assert!(!state.uploading);
}
