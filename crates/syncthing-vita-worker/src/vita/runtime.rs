//! newlib / pthread run-time of the module.
//!
//! An application's crt0 initializes newlib before `main` (`_init_vita_newlib`: heap, per-thread
//! reentrancy, malloc lock, file descriptors) and runs the C constructors (`__libc_init_array`,
//! which also calls pthread-embedded's `pthread_init`). A module has no crt0: `start` does the
//! same in `module_start`, `stop` undoes it in `module_stop` so that everything newlib holds
//! (its heap memory block and its kernel objects) is given back to `SceShell` on unload.

use super::ffi;

/// Initializes the run-time. Returns `false` (everything undone) if the heap memory block
/// (`_newlib_heap_size_user` bytes) could not be allocated in `SceShell`.
///
/// # Safety
/// Called once, from `module_start`, before any allocation.
pub unsafe fn start() -> bool {
    ffi::_init_vita_heap();
    ffi::_init_vita_reent();
    ffi::_init_vita_malloc();
    ffi::_init_vita_io();
    // `_init_vita_heap` leaves the heap empty when `sceKernelAllocMemBlock` fails: the first
    // allocation would then abort. `sbrk(0)` returns -1 in that case.
    if ffi::sbrk(0) as isize == -1 {
        free_newlib();
        return false;
    }
    ffi::__libc_init_array();
    true
}

/// Releases the run-time.
///
/// # Safety
/// Called once, from `module_stop`, after every thread of the module ended.
pub unsafe fn stop() {
    ffi::__libc_fini_array();
    ffi::pthread_terminate();
    for lock in [
        ffi::pte_thread_reuse_lock,
        ffi::pte_mutex_test_init_lock,
        ffi::pte_cond_list_lock,
        ffi::pte_cond_test_init_lock,
        ffi::pte_rwlock_test_init_lock,
        ffi::pte_spinlock_test_init_lock,
    ] {
        if lock > 0 {
            ffi::sceKernelDeleteMutex(lock);
        }
    }
    free_newlib();
}

unsafe fn free_newlib() {
    ffi::_free_vita_io();
    ffi::_free_vita_malloc();
    ffi::_free_vita_reent();
    ffi::_free_vita_heap();
}
