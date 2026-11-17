//! The watcher turning the abort signal and the deadline into a stop request.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use syncthing_core::SyncControl;
use syncthing_vita_worker::abort::{never_abort, AbortSignal, CycleWatcher, StopCause};

#[test]
fn test_finish_without_a_cause() {
    let control = SyncControl::new();
    let watcher = CycleWatcher::start(never_abort(), Arc::clone(&control), Duration::from_secs(60));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(watcher.finish(), None);
    assert!(!control.is_stop_requested());
}

#[test]
fn test_abort_requests_a_stop() {
    let control = SyncControl::new();
    let fire = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&fire);
    let abort: AbortSignal = Arc::new(move || flag.load(Ordering::SeqCst));
    let watcher = CycleWatcher::start(abort, Arc::clone(&control), Duration::from_secs(60));
    std::thread::sleep(Duration::from_millis(300));
    assert!(!control.is_stop_requested());
    fire.store(true, Ordering::SeqCst);
    let started = Instant::now();
    while !control.is_stop_requested() && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(control.is_stop_requested());
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "polled every 200 ms"
    );
    assert_eq!(watcher.finish(), Some(StopCause::Aborted));
}

#[test]
fn test_deadline_requests_a_stop() {
    let control = SyncControl::new();
    let started = Instant::now();
    let watcher = CycleWatcher::start(
        never_abort(),
        Arc::clone(&control),
        Duration::from_millis(500),
    );
    while !control.is_stop_requested() && started.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(500) && elapsed < Duration::from_millis(1200));
    assert_eq!(watcher.finish(), Some(StopCause::DeadlineReached));
    assert!(!(never_abort())());
}
