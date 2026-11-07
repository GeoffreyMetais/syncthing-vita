//! PS Vita system initialization (`SceSysmodule`, `SceSysmem`, `SceNet`, `SceNetCtl`, `SceLibKernel`).

#[cfg(target_os = "vita")]
const NET_INIT_SIZE: usize = 1024 * 1024;

#[cfg(target_os = "vita")]
#[allow(dead_code)]
pub mod vita_ffi {
    use libc::{c_char, c_int, c_ushort, c_void};

    pub const SCE_SYSMODULE_NET: c_ushort = 0x0001;
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_RW: u32 = 0x0C20_D060;

    #[repr(C)]
    pub struct SceNetInitParam {
        pub memory: *mut c_void,
        pub size: c_int,
        pub flags: c_int,
    }

    #[link(name = "SceSysmodule_stub")]
    extern "C" {
        pub fn sceSysmoduleLoadModule(id: c_ushort) -> c_int;
    }

    #[link(name = "SceLibKernel_stub")]
    extern "C" {
        pub fn sceKernelGetRandomNumber(output: *mut c_void, size: u32) -> c_int;
    }

    #[link(name = "SceSysmem_stub")]
    extern "C" {
        pub fn sceKernelAllocMemBlock(
            name: *const c_char,
            memblock_type: u32,
            size: c_int,
            opt: *const c_void,
        ) -> c_int;
        pub fn sceKernelGetMemBlockBase(uid: c_int, basep: *mut *mut c_void) -> c_int;
    }

    #[link(name = "SceNet_stub")]
    extern "C" {
        pub fn sceNetInit(param: *mut SceNetInitParam) -> c_int;
        pub fn sceNetTerm() -> c_int;
    }

    #[link(name = "SceNetCtl_stub")]
    extern "C" {
        pub fn sceNetCtlInit() -> c_int;
        pub fn sceNetCtlTerm();
    }
}

/// Initializes PS Vita's networking stack (`SCE_SYSMODULE_NET`, `SceNetMemory`, `sceNetInit`, `sceNetCtlInit`).
#[cfg(target_os = "vita")]
pub fn init_vita_network() -> Result<Vec<u8>, String> {
    use std::ptr;
    use vita_ffi::{
        sceKernelAllocMemBlock, sceKernelGetMemBlockBase, sceNetCtlInit, sceNetInit,
        sceSysmoduleLoadModule, SceNetInitParam, SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
        SCE_SYSMODULE_NET,
    };

    const SCE_NET_ERROR_EEXIST: u32 = 0x80410118;
    const SCE_NET_CTL_ERROR_NOT_TERMINATED: u32 = 0x80412102;

    unsafe {
        // Must load SCE_SYSMODULE_NET (0x0001) on real PS Vita hardware before calling sceNetInit
        let _ = sceSysmoduleLoadModule(SCE_SYSMODULE_NET);

        let mut net_memory = Vec::new();
        let mut mem_ptr: *mut libc::c_void = ptr::null_mut();

        let uid = sceKernelAllocMemBlock(
            b"SceNetMemory\0".as_ptr() as *const libc::c_char,
            SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
            NET_INIT_SIZE as i32,
            ptr::null(),
        );
        if uid >= 0 {
            let _ = sceKernelGetMemBlockBase(uid, &mut mem_ptr);
        }
        if mem_ptr.is_null() {
            net_memory = vec![0u8; NET_INIT_SIZE];
            mem_ptr = net_memory.as_mut_ptr() as *mut libc::c_void;
        }

        let mut init_param = SceNetInitParam {
            memory: mem_ptr,
            size: NET_INIT_SIZE as i32,
            flags: 0,
        };

        let ret = sceNetInit(&mut init_param);
        if ret < 0 && (ret as u32) != SCE_NET_ERROR_EEXIST {
            return Err(format!("sceNetInit failed: 0x{:08X}", ret as u32));
        }

        let ctl_ret = sceNetCtlInit();
        if ctl_ret < 0 && (ctl_ret as u32) != SCE_NET_CTL_ERROR_NOT_TERMINATED {
            // Non-blocking in Vita3K / if already initialized
        }

        Ok(net_memory)
    }
}

#[cfg(not(target_os = "vita"))]
pub fn init_vita_network() -> Result<Vec<u8>, String> {
    Ok(Vec::new())
}
