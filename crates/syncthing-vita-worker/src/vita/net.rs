//! The network stack inside `SceShell`.
//!
//! `SceShell` already runs `SceNet` / `SceNetCtl` (notifications, PSN), so the worker normally
//! only uses them. Should they not be initialized, the worker initializes them with its own
//! memory block and terminates them when its cycle ends — and only then: terminating the
//! shell's network stack would cut its connections.

use std::ptr;

use super::ffi;

/// Memory given to `sceNetInit` when the worker has to initialize the stack itself.
const NET_MEMORY_SIZE: i32 = 256 * 1024;

/// What the worker initialized (and must terminate).
pub struct NetSession {
    owns_net: bool,
    owns_ctl: bool,
    memblock: i32,
}

impl NetSession {
    /// Makes sure `SceNet` and `SceNetCtl` are usable. `Err` holds the failing return code.
    pub fn acquire() -> Result<Self, i32> {
        let mut session = Self {
            owns_net: false,
            owns_ctl: false,
            memblock: -1,
        };
        unsafe {
            if ffi::sceSysmoduleIsLoaded(ffi::SCE_SYSMODULE_NET) != 0 {
                let ret = ffi::sceSysmoduleLoadModule(ffi::SCE_SYSMODULE_NET);
                if ret < 0 {
                    return Err(ret);
                }
            }
            if ffi::sceNetShowNetstat() as u32 == ffi::SCE_NET_ERROR_ENOTINIT {
                session.init_net()?;
            }
            let ret = ffi::sceNetCtlInit();
            if ret >= 0 {
                session.owns_ctl = true;
            } else if ret as u32 != ffi::SCE_NETCTL_ERROR_NOT_TERMINATED {
                return Err(ret);
            }
        }
        Ok(session)
    }

    unsafe fn init_net(&mut self) -> Result<(), i32> {
        self.memblock = ffi::sceKernelAllocMemBlock(
            c"SyncthingWorkerNet".as_ptr(),
            ffi::SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
            NET_MEMORY_SIZE,
            ptr::null(),
        );
        if self.memblock < 0 {
            return Err(self.memblock);
        }
        let mut base = ptr::null_mut();
        let ret = ffi::sceKernelGetMemBlockBase(self.memblock, &mut base);
        if ret < 0 {
            return Err(ret);
        }
        let mut param = ffi::SceNetInitParam {
            memory: base,
            size: NET_MEMORY_SIZE,
            flags: 0,
        };
        let ret = ffi::sceNetInit(&mut param);
        if ret >= 0 {
            self.owns_net = true;
        } else if ret as u32 != ffi::SCE_NET_ERROR_EBUSY {
            return Err(ret);
        }
        Ok(())
    }
}

impl Drop for NetSession {
    fn drop(&mut self) {
        unsafe {
            if self.owns_ctl {
                ffi::sceNetCtlTerm();
            }
            if self.owns_net {
                ffi::sceNetTerm();
            }
            // Freed only once `sceNetTerm` no longer uses it (or the stack never took it).
            if self.memblock >= 0 {
                ffi::sceKernelFreeMemBlock(self.memblock);
            }
        }
    }
}
