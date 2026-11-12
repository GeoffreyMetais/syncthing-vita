//! Sequential hand-over between background workers: a new worker only starts once the
//! previous one has fully stopped.
//!
//! Restarting the sync (configuration change, manual sync, reconnection) asks the running
//! worker to stop and spawns a new one right away. Stopping takes a moment (network timeouts,
//! a block being written), so without this hand-over two generations of sessions would
//! overlap and could write the same download temp file at the same time.

use std::io;
use std::sync::Mutex;
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

/// Poll interval of [`wait_for_worker`].
const WAIT_POLL: Duration = Duration::from_millis(20);

/// The most recently spawned worker.
static CURRENT_WORKER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Spawns a worker thread running `body` after the previously spawned worker has ended.
/// The wait happens inside the new thread, so the caller (UI thread) never blocks.
/// `on_wait` runs first, in the new thread, when the previous worker is still running.
pub fn spawn_after_previous<W, F>(
    name: &str,
    stack_size: usize,
    on_wait: W,
    body: F,
) -> io::Result<()>
where
    W: FnOnce() + Send + 'static,
    F: FnOnce() + Send + 'static,
{
    let mut slot = CURRENT_WORKER.lock().unwrap_or_else(|p| p.into_inner());
    let previous = slot.take();
    let handle = Builder::new()
        .name(name.to_owned())
        .stack_size(stack_size)
        .spawn(move || {
            if let Some(previous) = previous {
                if !previous.is_finished() {
                    on_wait();
                }
                let _ = previous.join();
            }
            body();
        })?;
    *slot = Some(handle);
    Ok(())
}

/// Waits up to `max_wait` for the current worker (and those it was waiting for) to end.
/// Returns `true` if no worker is running anymore.
pub fn wait_for_worker(max_wait: Duration) -> bool {
    let mut slot = CURRENT_WORKER.lock().unwrap_or_else(|p| p.into_inner());
    let Some(handle) = slot.take() else {
        return true;
    };
    let deadline = Instant::now() + max_wait;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            *slot = Some(handle);
            return false;
        }
        std::thread::sleep(WAIT_POLL);
    }
    let _ = handle.join();
    true
}
