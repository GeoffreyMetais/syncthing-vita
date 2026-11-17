//! System and runtime symbols used by the module shim (VitaSDK stubs, newlib, pthread-embedded).

use libc::{c_char, c_int, c_uint, c_void};

pub const SCE_KERNEL_START_SUCCESS: c_int = 0;
pub const SCE_KERNEL_START_FAILED: c_int = 2;
pub const SCE_KERNEL_STOP_SUCCESS: c_int = 0;
pub const SCE_KERNEL_STOP_CANCEL: c_int = 1;

pub const SCE_SYSMODULE_NET: u16 = 0x0001;
pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_RW: c_uint = 0x0C20_D060;
/// `sceNetShowNetstat` before `sceNetInit` (psp2common/net.h).
pub const SCE_NET_ERROR_ENOTINIT: u32 = 0x8041_01C8;
/// `sceNetInit` when the stack is already initialized (psp2common/net.h).
pub const SCE_NET_ERROR_EBUSY: u32 = 0x8041_0110;
/// `sceNetCtlInit` when it is already initialized (also what newlib checks).
pub const SCE_NETCTL_ERROR_NOT_TERMINATED: u32 = 0x8041_2102;

#[repr(C)]
pub struct SceNetInitParam {
    pub memory: *mut c_void,
    pub size: c_int,
    pub flags: c_int,
}

#[link(name = "SceSysmodule_stub")]
extern "C" {
    pub fn sceSysmoduleIsLoaded(id: u16) -> c_int;
    pub fn sceSysmoduleLoadModule(id: u16) -> c_int;
}

#[link(name = "SceSysmem_stub")]
extern "C" {
    pub fn sceKernelAllocMemBlock(
        name: *const c_char,
        memblock_type: c_uint,
        size: c_int,
        opt: *const c_void,
    ) -> c_int;
    pub fn sceKernelGetMemBlockBase(uid: c_int, basep: *mut *mut c_void) -> c_int;
    pub fn sceKernelFreeMemBlock(uid: c_int) -> c_int;
}

#[link(name = "SceNet_stub")]
extern "C" {
    pub fn sceNetShowNetstat() -> c_int;
    pub fn sceNetInit(param: *mut SceNetInitParam) -> c_int;
    pub fn sceNetTerm() -> c_int;
}

#[link(name = "SceNetCtl_stub")]
extern "C" {
    pub fn sceNetCtlInit() -> c_int;
    pub fn sceNetCtlTerm();
}

#[link(name = "SceKernelThreadMgr_stub")]
extern "C" {
    pub fn sceKernelExitDeleteThread(status: c_int) -> c_int;
    pub fn sceKernelDeleteMutex(uid: c_int) -> c_int;
}

// newlib (VitaSDK libc.a): what crt0's `_init_vita_newlib` / `_free_vita_newlib` call, plus
// the C runtime constructors. A module has no crt0, so the shim calls them itself.
extern "C" {
    pub fn _init_vita_heap();
    pub fn _init_vita_reent();
    pub fn _init_vita_malloc();
    pub fn _init_vita_io();
    pub fn _free_vita_io();
    pub fn _free_vita_malloc();
    pub fn _free_vita_reent();
    pub fn _free_vita_heap();
    pub fn __libc_init_array();
    pub fn __libc_fini_array();
    pub fn sbrk(increment: libc::intptr_t) -> *mut c_void;
}

// pthread-embedded (VitaSDK libpthread.a): `pthread_init` runs as a constructor
// (`pthread_setup`); `pthread_terminate` frees its keys but leaves these kernel mutexes.
extern "C" {
    pub fn pthread_terminate();
    pub static pte_thread_reuse_lock: c_int;
    pub static pte_mutex_test_init_lock: c_int;
    pub static pte_cond_list_lock: c_int;
    pub static pte_cond_test_init_lock: c_int;
    pub static pte_rwlock_test_init_lock: c_int;
    pub static pte_spinlock_test_init_lock: c_int;
}

/// `SCE_SYSMODULE_NOTIFICATION_UTIL` (psp2/sysmodule.h).
pub const SCE_SYSMODULE_NOTIFICATION_UTIL: u16 = 0x0033;

// Weak imports: the module still loads if a library is missing; the shim only calls them once
// it checked the library is there (the launcher loads `SceNotificationUtil` before the worker).
#[link(name = "SceNotificationUtil_stub_weak")]
extern "C" {
    /// `text`: 0x410 bytes of NUL-terminated UTF-16 (psp2/notificationutil.h).
    pub fn sceNotificationUtilSendNotification(text: *const u16) -> c_int;
}

#[link(name = "SceRegistryMgr_stub_weak")]
extern "C" {
    pub fn sceRegMgrGetKeyInt(
        category: *const c_char,
        name: *const c_char,
        buf: *mut c_int,
    ) -> c_int;
}
