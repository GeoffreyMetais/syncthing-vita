//! Worker hand-over: a new background worker never runs while the previous one is still alive.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use vita_syncthing_app::infrastructure::wait_for_worker;
use vita_syncthing_app::infrastructure::worker_slot::spawn_after_previous;

const STACK: usize = 256 * 1024;

#[test]
fn test_new_worker_starts_only_after_previous_one_ended() {
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
