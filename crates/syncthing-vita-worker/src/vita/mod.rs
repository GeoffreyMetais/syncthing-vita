//! PS Vita module shim of `syncthing_worker.suprx`: entry points, run-time, network and the
//! link with the launcher. Everything else is the host-tested cycle ([`crate::run_cycle`]).
//!
//! Life of a worker (all inside `SceShell`, see AGENTS.md §8):
//! 1. the launcher calls `sceKernelLoadStartModule` with a pointer to its `WorkerLink`;
//! 2. `module_start` (on the launcher's thread) sets up newlib and pthread, then starts the
//!    cycle thread and returns at once;
//! 3. the cycle thread runs one cycle, stores its result and `STATE_DONE`;
//! 4. the launcher calls `sceKernelStopUnloadModule`: `module_stop` joins the cycle thread and
//!    releases the run-time (heap memory block, kernel objects) before the code is unloaded.
//!
//! A fatal error (allocation failure, panic: both end in `abort`) must never take `SceShell`
//! down: [`abort`] marks the link `STATE_FATAL` and ends only the failing thread. The launcher
//! then keeps the module loaded (unloading it under a thread could crash the shell) and stops
//! background sync until the next reboot.

mod ffi;
mod net;
mod runtime;

use std::cell::UnsafeCell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use libc::{c_int, c_uint, c_void};

use crate::abort::AbortSignal;
use crate::cycle::{run_cycle, CycleOptions};
use crate::defaults::{CYCLE_THREAD_STACK, VITA_DATA_DIR};
use crate::link::{
    outcome_code, WorkerLink, RESULT_BAD_LINK, RESULT_NO_MEMORY, RESULT_NO_NETWORK,
    RESULT_NO_THREAD, STATE_DONE, STATE_FATAL, STATE_RUNNING,
};
use crate::log::WorkerLog;
use net::NetSession;

/// Size of the newlib heap: one memory block allocated from `SceShell`'s budget at
/// `module_start` and freed at `module_stop` (newlib's `_init_vita_heap` reads it; without it
/// newlib would ask for 128 MiB and fail). See AGENTS.md §8 for the budget.
pub const WORKER_HEAP_BYTES: u32 = 8 * 1024 * 1024;

#[used]
#[no_mangle]
pub static _newlib_heap_size_user: u32 = WORKER_HEAP_BYTES;

/// The launcher's link, for the fatal error path.
static LINK: AtomicPtr<WorkerLink> = AtomicPtr::new(std::ptr::null_mut());
/// Set by [`abort`]: the other worker threads stop as soon as they see it.
static FATAL: AtomicBool = AtomicBool::new(false);

/// The cycle thread, joined by `module_stop`. Only touched by `module_start` / `module_stop`,
/// which the module manager calls one after the other from the launcher's thread.
struct CycleThread(UnsafeCell<Option<JoinHandle<()>>>);
// SAFETY: see above, never accessed concurrently.
unsafe impl Sync for CycleThread {}
static CYCLE_THREAD: CycleThread = CycleThread(UnsafeCell::new(None));

/// The link the launcher passed: `argp` points to a `SyncthingWorkerLink *`.
unsafe fn link_from_args(args: c_uint, argp: *const c_void) -> Option<&'static WorkerLink> {
    if argp.is_null() || (args as usize) < std::mem::size_of::<*const WorkerLink>() {
        return None;
    }
    let link = *(argp as *const *const WorkerLink);
    link.as_ref()
}

/// # Safety
/// Called by the module manager only.
#[no_mangle]
pub unsafe extern "C" fn module_start(args: c_uint, argp: *const c_void) -> c_int {
    let Some(link) = link_from_args(args, argp) else {
        return ffi::SCE_KERNEL_START_FAILED;
    };
    if !link.is_compatible() {
        link.result.store(RESULT_BAD_LINK, Ordering::SeqCst);
        return ffi::SCE_KERNEL_START_FAILED;
    }
    if !runtime::start() {
        link.result.store(RESULT_NO_MEMORY, Ordering::SeqCst);
        return ffi::SCE_KERNEL_START_FAILED;
    }
    LINK.store(
        link as *const WorkerLink as *mut WorkerLink,
        Ordering::SeqCst,
    );
    link.state.store(STATE_RUNNING, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("bg_sync_cycle".to_owned())
        .stack_size(CYCLE_THREAD_STACK)
        .spawn(move || cycle_thread(link));
    match spawned {
        Ok(handle) => {
            *CYCLE_THREAD.0.get() = Some(handle);
            ffi::SCE_KERNEL_START_SUCCESS
        }
        Err(_) => {
            link.result.store(RESULT_NO_THREAD, Ordering::SeqCst);
            LINK.store(std::ptr::null_mut(), Ordering::SeqCst);
            runtime::stop();
            ffi::SCE_KERNEL_START_FAILED
        }
    }
}

/// # Safety
/// Called by the module manager only.
#[no_mangle]
pub unsafe extern "C" fn module_stop(_args: c_uint, _argp: *const c_void) -> c_int {
    let link = LINK.load(Ordering::SeqCst);
    // Only a finished cycle may be unloaded: its thread is about to return.
    if link.is_null() || (*link).state.load(Ordering::SeqCst) != STATE_DONE {
        return ffi::SCE_KERNEL_STOP_CANCEL;
    }
    if let Some(handle) = (*CYCLE_THREAD.0.get()).take() {
        let _ = handle.join();
    }
    LINK.store(std::ptr::null_mut(), Ordering::SeqCst);
    runtime::stop();
    ffi::SCE_KERNEL_STOP_SUCCESS
}

fn cycle_thread(link: &'static WorkerLink) {
    let abort: AbortSignal =
        Arc::new(move || link.abort.load(Ordering::SeqCst) != 0 || FATAL.load(Ordering::SeqCst));
    let data_dir = Path::new(VITA_DATA_DIR);
    let result = match NetSession::acquire() {
        Ok(net) => {
            let report = run_cycle(data_dir, abort, &CycleOptions::background());
            drop(net);
            outcome_code(&report.outcome)
        }
        Err(code) => {
            WorkerLog::new(data_dir).line(&format!("network unavailable (0x{:08X})", code as u32));
            RESULT_NO_NETWORK
        }
    };
    link.result.store(result, Ordering::SeqCst);
    // Never hides a fatal error of another worker thread.
    let _ = link.state.compare_exchange(
        STATE_RUNNING,
        STATE_DONE,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}

/// Replaces newlib's `abort` (Rust's panics and allocation failures end here): marks the link
/// `STATE_FATAL` and ends the calling thread instead of `SceShell`.
#[no_mangle]
pub extern "C" fn abort() -> ! {
    end_thread_on_fatal_error()
}

/// crt0's `_free_vita_newlib`, which newlib's `_exit` calls before `sceKernelExitProcess`
/// (that would terminate `SceShell` itself). A module has no crt0: this definition makes any
/// `exit` / `_exit` reached in the worker a fatal error of the calling thread instead, and
/// never returns to `sceKernelExitProcess`. The shim releases newlib with `runtime::stop`.
#[no_mangle]
pub extern "C" fn _free_vita_newlib() {
    end_thread_on_fatal_error()
}

fn end_thread_on_fatal_error() -> ! {
    FATAL.store(true, Ordering::SeqCst);
    let link = LINK.load(Ordering::SeqCst);
    // SAFETY: the launcher keeps the link alive while the module is loaded.
    if let Some(link) = unsafe { link.as_ref() } {
        link.state.store(STATE_FATAL, Ordering::SeqCst);
    }
    loop {
        // SAFETY: ends the calling thread only.
        unsafe {
            ffi::sceKernelExitDeleteThread(0);
        }
    }
}
