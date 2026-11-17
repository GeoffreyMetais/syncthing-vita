//! `BackgroundHandover`: the app's first sync worker waits while a background worker of the
//! plugin runs, at most `BACKGROUND_WORKER_WAIT`, then never again.

use std::time::{Duration, Instant};

use vita_syncthing_app::application::{BackgroundHandover, HandoverStep, ReleaseReason};
use vita_syncthing_app::domain::defaults::BACKGROUND_WORKER_WAIT;

#[test]
fn test_wait_bound() {
    assert_eq!(BACKGROUND_WORKER_WAIT, Duration::from_secs(30));
}

#[test]
fn test_released_at_once_without_a_background_worker() {
    let mut gate = BackgroundHandover::new();
    assert!(!gate.is_released());
    let now = Instant::now();
    assert_eq!(
        gate.check(false, now),
        HandoverStep::Released(ReleaseReason::NoWorker)
    );
    assert!(gate.is_released());
    // A worker appearing later changes nothing: only the first start waits.
    assert_eq!(gate.check(true, now), HandoverStep::AlreadyReleased);
    assert_eq!(gate.check(false, now), HandoverStep::AlreadyReleased);
}

#[test]
fn test_waits_while_the_worker_runs_then_reports_how_long() {
    let mut gate = BackgroundHandover::new();
    let t0 = Instant::now();
    assert_eq!(gate.check(true, t0), HandoverStep::Wait);
    assert_eq!(
        gate.check(true, t0 + Duration::from_secs(5)),
        HandoverStep::Wait
    );
    assert!(!gate.is_released());
    assert_eq!(
        gate.check(false, t0 + Duration::from_millis(7250)),
        HandoverStep::Released(ReleaseReason::Stopped(Duration::from_millis(7250)))
    );
    assert!(gate.is_released());
}

#[test]
fn test_gives_up_exactly_at_the_bound() {
    let mut gate = BackgroundHandover::new();
    let t0 = Instant::now();
    assert_eq!(gate.check(true, t0), HandoverStep::Wait);
    let just_before = t0 + BACKGROUND_WORKER_WAIT - Duration::from_millis(1);
    assert_eq!(gate.check(true, just_before), HandoverStep::Wait);
    assert_eq!(
        gate.check(true, t0 + BACKGROUND_WORKER_WAIT),
        HandoverStep::Released(ReleaseReason::TimedOut)
    );
    assert!(gate.is_released());
    assert_eq!(
        gate.check(true, t0 + BACKGROUND_WORKER_WAIT * 2),
        HandoverStep::AlreadyReleased
    );
}

#[test]
fn test_wait_is_counted_from_the_first_running_check() {
    let mut gate = BackgroundHandover::new();
    let t0 = Instant::now();
    assert_eq!(
        gate.check(true, t0 + Duration::from_secs(100)),
        HandoverStep::Wait,
        "the clock starts at the first check, not at creation"
    );
    assert_eq!(
        gate.check(true, t0 + Duration::from_secs(129)),
        HandoverStep::Wait
    );
    // A clock that goes backwards never panics nor releases.
    assert_eq!(gate.check(true, t0), HandoverStep::Wait);
    assert_eq!(
        gate.check(false, t0),
        HandoverStep::Released(ReleaseReason::Stopped(Duration::ZERO))
    );
}
