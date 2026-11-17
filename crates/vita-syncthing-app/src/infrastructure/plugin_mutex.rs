//! Named kernel mutexes shared with the background sync plugin (`SceKernelThreadMgr`).
//!
//! - `SyncthingGuiRunning`: created locked by this app at startup and held for its whole
//!   lifetime (`GuiRunningLock`). The plugin never syncs while it exists, so the app and the
//!   plugin never write the same folders or databases at the same time. The kernel deletes it
//!   if the app dies.
//! - `SyncthingLauncherAlive`: created by the launcher at `module_start`. Opening it tells
//!   whether the launcher is loaded (`KernelLauncherProbe`).
//! - `SyncthingWorkerRunning`: held by the launcher while a background worker module is loaded.
//!   The app's first sync worker waits for it to disappear (`KernelWorkerProbe`,
//!   `application::BackgroundHandover`).
//!
//! Host builds have no kernel objects: the lock is a no-op.

/// Name of the mutex this app holds while it runs.
pub const GUI_RUNNING_MUTEX: &str = "SyncthingGuiRunning";
/// Name of the mutex the plugin's launcher creates once loaded.
pub const LAUNCHER_ALIVE_MUTEX: &str = "SyncthingLauncherAlive";
/// Name of the mutex the launcher holds while a background worker module is loaded.
pub const WORKER_RUNNING_MUTEX: &str = "SyncthingWorkerRunning";

/// Tells whether a background worker of the plugin is loaded (`SyncthingWorkerRunning`).
pub trait BackgroundWorkerProbe {
    fn worker_running(&self) -> bool;
}

/// Probe of a system without the plugin (host builds): no background worker ever runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBackgroundWorker;

impl BackgroundWorkerProbe for NoBackgroundWorker {
    fn worker_running(&self) -> bool {
        false
    }
}

/// Probe of the running system: `SyncthingWorkerRunning` on the PS Vita, none on host builds.
pub fn system_worker_probe() -> Box<dyn BackgroundWorkerProbe> {
    #[cfg(target_os = "vita")]
    {
        Box::new(KernelWorkerProbe)
    }
    #[cfg(not(target_os = "vita"))]
    {
        Box::new(NoBackgroundWorker)
    }
}

#[cfg(target_os = "vita")]
mod ffi {
    use libc::{c_char, c_int, c_uint, c_void};

    /// `SCE_KERNEL_ATTR_OPENABLE`: other processes may open the object by name.
    pub const SCE_KERNEL_ATTR_OPENABLE: c_uint = 0x80;

    #[link(name = "SceKernelThreadMgr_stub")]
    extern "C" {
        pub fn sceKernelCreateMutex(
            name: *const c_char,
            attr: c_uint,
            init_count: c_int,
            option: *const c_void,
        ) -> c_int;
        pub fn sceKernelDeleteMutex(uid: c_int) -> c_int;
        pub fn sceKernelUnlockMutex(uid: c_int, unlock_count: c_int) -> c_int;
        pub fn sceKernelOpenMutex(name: *const c_char) -> c_int;
        pub fn sceKernelCloseMutex(uid: c_int) -> c_int;
    }

    /// NUL-terminated copy of `name`.
    pub fn c_name(name: &str) -> Vec<u8> {
        let mut out = name.as_bytes().to_vec();
        out.push(0);
        out
    }
}

/// `SyncthingGuiRunning`, locked by this app until dropped (end of `main`).
pub struct GuiRunningLock {
    #[cfg(target_os = "vita")]
    uid: libc::c_int,
}

impl GuiRunningLock {
    /// Creates the mutex locked by the calling thread (`init_count = 1`). A failure is not
    /// fatal for the app (the plugin also checks that title `SYNCTHING` is not running).
    #[cfg(target_os = "vita")]
    pub fn acquire() -> Self {
        let name = ffi::c_name(GUI_RUNNING_MUTEX);
        let uid = unsafe {
            ffi::sceKernelCreateMutex(
                name.as_ptr() as *const libc::c_char,
                ffi::SCE_KERNEL_ATTR_OPENABLE,
                1,
                std::ptr::null(),
            )
        };
        Self { uid }
    }

    #[cfg(not(target_os = "vita"))]
    pub fn acquire() -> Self {
        Self {}
    }

    /// True if the mutex exists (always false on host builds).
    pub fn is_held(&self) -> bool {
        #[cfg(target_os = "vita")]
        {
            self.uid >= 0
        }
        #[cfg(not(target_os = "vita"))]
        {
            false
        }
    }
}

impl Drop for GuiRunningLock {
    fn drop(&mut self) {
        #[cfg(target_os = "vita")]
        if self.uid >= 0 {
            unsafe {
                ffi::sceKernelUnlockMutex(self.uid, 1);
                ffi::sceKernelDeleteMutex(self.uid);
            }
        }
    }
}

/// Opens `SyncthingLauncherAlive` to tell whether the launcher is loaded.
///
/// With the `ui-script` feature (emulator smoke tests: Vita3K has no taiHEN), the file
/// `ux0:data/syncthing/ui_script_launcher_alive` holding `1` or `0` overrides the answer.
#[cfg(target_os = "vita")]
pub struct KernelLauncherProbe;

#[cfg(all(target_os = "vita", feature = "ui-script"))]
const UI_SCRIPT_PROBE_FILE: &str = "ux0:data/syncthing/ui_script_launcher_alive";

#[cfg(target_os = "vita")]
impl super::plugin_host::LauncherProbe for KernelLauncherProbe {
    fn launcher_alive(&self) -> bool {
        #[cfg(feature = "ui-script")]
        if let Ok(forced) = std::fs::read_to_string(UI_SCRIPT_PROBE_FILE) {
            return forced.trim() == "1";
        }
        mutex_exists(LAUNCHER_ALIVE_MUTEX)
    }
}

/// True if the named openable mutex `name` exists (the handle is closed at once).
#[cfg(target_os = "vita")]
fn mutex_exists(name: &str) -> bool {
    let name = ffi::c_name(name);
    let uid = unsafe { ffi::sceKernelOpenMutex(name.as_ptr() as *const libc::c_char) };
    if uid < 0 {
        return false;
    }
    unsafe {
        ffi::sceKernelCloseMutex(uid);
    }
    true
}

/// Opens `SyncthingWorkerRunning` to tell whether a background worker is loaded.
///
/// With the `ui-script` feature, the file `ux0:data/syncthing/ui_script_worker_running`
/// holding `1` or `0` overrides the answer (Vita3K has no taiHEN).
#[cfg(target_os = "vita")]
pub struct KernelWorkerProbe;

#[cfg(all(target_os = "vita", feature = "ui-script"))]
const UI_SCRIPT_WORKER_FILE: &str = "ux0:data/syncthing/ui_script_worker_running";

#[cfg(target_os = "vita")]
impl BackgroundWorkerProbe for KernelWorkerProbe {
    fn worker_running(&self) -> bool {
        #[cfg(feature = "ui-script")]
        if let Ok(forced) = std::fs::read_to_string(UI_SCRIPT_WORKER_FILE) {
            return forced.trim() == "1";
        }
        mutex_exists(WORKER_RUNNING_MUTEX)
    }
}
