//! Turning the launcher's abort request (a game or the app starts, the plugin is disabled)
//! and the cycle's deadline into a stop request of the running session.
//!
//! The sessions only know `SyncControl::request_stop`, checked between blocks and phases, so
//! a watcher thread polls the abort signal every `ABORT_POLL` while the cycle runs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use syncthing_core::SyncControl;

use crate::defaults::{ABORT_POLL, WATCHER_THREAD_STACK};

/// Tells the cycle whether it must stop. On the PS Vita it reads the flag the launcher shares
/// with the worker; in tests, any closure.
pub type AbortSignal = Arc<dyn Fn() -> bool + Send + Sync>;

/// An abort signal that never fires.
pub fn never_abort() -> AbortSignal {
    Arc::new(|| false)
}

/// Why the watcher stopped the cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    Aborted,
    DeadlineReached,
}

/// Watches the abort signal and the deadline of a cycle while it runs.
pub struct CycleWatcher {
    done: Arc<AtomicBool>,
    handle: Option<JoinHandle<Option<StopCause>>>,
}

impl CycleWatcher {
    /// Starts watching: once `abort` fires or `max_duration` elapsed, `control` is asked to
    /// stop. Without a thread (spawn failure), the cycle still ends at its own pace.
    pub fn start(abort: AbortSignal, control: Arc<SyncControl>, max_duration: Duration) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        let started = Instant::now();
        let handle = thread::Builder::new()
            .name("bg_sync_watch".to_owned())
            .stack_size(WATCHER_THREAD_STACK)
            .spawn(move || watch(&*abort, &control, &flag, started, max_duration))
            .ok();
        Self { done, handle }
    }

    /// Ends the watcher and tells whether it stopped the cycle, and why.
    pub fn finish(mut self) -> Option<StopCause> {
        self.done.store(true, Ordering::SeqCst);
        self.handle.take().and_then(|h| h.join().ok()).flatten()
    }
}

fn watch(
    abort: &dyn Fn() -> bool,
    control: &SyncControl,
    done: &AtomicBool,
    started: Instant,
    max_duration: Duration,
) -> Option<StopCause> {
    while !done.load(Ordering::SeqCst) {
        let cause = if abort() {
            Some(StopCause::Aborted)
        } else if started.elapsed() >= max_duration {
            Some(StopCause::DeadlineReached)
        } else {
            None
        };
        if cause.is_some() {
            control.request_stop();
            return cause;
        }
        thread::sleep(ABORT_POLL);
    }
    None
}
