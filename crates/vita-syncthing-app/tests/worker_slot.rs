//! Worker hand-over: a new background worker never runs while the previous one is still alive.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use vita_syncthing_app::infrastructure::wait_for_worker;
use vita_syncthing_app::infrastructure::worker_slot::spawn_after_previous;

const STACK: usize = 256 * 1024;

/// The worker slot is process-wide: tests of this file run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

#[test]
fn test_new_worker_starts_only_after_previous_one_ended() {
    let _serial = serial();
    let first_done = Arc::new(AtomicBool::new(false));
    let waited = Arc::new(AtomicBool::new(false));
    let overlapped = Arc::new(AtomicBool::new(false));

    let done = Arc::clone(&first_done);
    spawn_after_previous(
        "first",
        STACK,
        || {},
        move || {
            std::thread::sleep(Duration::from_millis(300));
            done.store(true, Ordering::SeqCst);
        },
    )
    .unwrap();

    let (done, waited_flag, overlap) = (
        Arc::clone(&first_done),
        Arc::clone(&waited),
        Arc::clone(&overlapped),
    );
    spawn_after_previous(
        "second",
        STACK,
        move || waited_flag.store(true, Ordering::SeqCst),
        move || overlap.store(!done.load(Ordering::SeqCst), Ordering::SeqCst),
    )
    .unwrap();

    // The first worker alone outlasts this short wait.
    assert!(!wait_for_worker(Duration::from_millis(20)));
    assert!(wait_for_worker(Duration::from_secs(5)));
    assert!(first_done.load(Ordering::SeqCst));
    assert!(waited.load(Ordering::SeqCst));
    assert!(!overlapped.load(Ordering::SeqCst));
    // Nothing left to wait for.
    assert!(wait_for_worker(Duration::ZERO));
}

#[test]
fn test_wait_hook_runs_only_while_the_previous_worker_is_alive() {
    let _serial = serial();
    assert!(wait_for_worker(Duration::ZERO));

    // The first worker has already ended when the second one is spawned.
    spawn_after_previous("quick", STACK, || {}, || {}).unwrap();
    std::thread::sleep(Duration::from_millis(150));

    let waited = Arc::new(AtomicBool::new(false));
    let ran = Arc::new(AtomicBool::new(false));
    let (waited_flag, ran_flag) = (Arc::clone(&waited), Arc::clone(&ran));
    spawn_after_previous(
        "after-quick",
        STACK,
        move || waited_flag.store(true, Ordering::SeqCst),
        move || ran_flag.store(true, Ordering::SeqCst),
    )
    .unwrap();
    assert!(wait_for_worker(Duration::from_secs(5)));
    assert!(ran.load(Ordering::SeqCst), "the body runs");
    assert!(
        !waited.load(Ordering::SeqCst),
        "nothing to wait for: no wait hook"
    );
}

#[test]
fn test_wait_hook_runs_before_the_body_while_the_previous_worker_runs() {
    let _serial = serial();
    assert!(wait_for_worker(Duration::ZERO));

    spawn_after_previous(
        "long",
        STACK,
        || {},
        || std::thread::sleep(Duration::from_millis(200)),
    )
    .unwrap();

    let order = Arc::new(Mutex::new(Vec::new()));
    let (wait_order, body_order) = (Arc::clone(&order), Arc::clone(&order));
    spawn_after_previous(
        "next",
        STACK,
        move || wait_order.lock().unwrap().push("wait"),
        move || body_order.lock().unwrap().push("body"),
    )
    .unwrap();
    assert!(wait_for_worker(Duration::from_secs(5)));
    assert_eq!(*order.lock().unwrap(), vec!["wait", "body"]);
}

#[test]
fn test_wait_for_worker_gives_up_and_keeps_the_worker_for_later() {
    let _serial = serial();
    assert!(wait_for_worker(Duration::ZERO));

    spawn_after_previous(
        "slow",
        STACK,
        || {},
        || std::thread::sleep(Duration::from_millis(400)),
    )
    .unwrap();
    // Too short: the worker is still running, and a later wait still finds it.
    assert!(!wait_for_worker(Duration::from_millis(10)));
    assert!(!wait_for_worker(Duration::ZERO));
    assert!(wait_for_worker(Duration::from_secs(5)));
    assert!(wait_for_worker(Duration::ZERO));
}
