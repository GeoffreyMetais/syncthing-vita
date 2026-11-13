//! PS Vita system initialization (`SceSysmodule`, `SceSysmem`, `SceNet`, `SceNetCtl`, `SceLibKernel`, `SceAppUtil`).

use crate::domain::{Language, WifiLink};

#[cfg(target_os = "vita")]
const NET_INIT_SIZE: usize = 1024 * 1024;

#[cfg(target_os = "vita")]
#[allow(dead_code)]
pub mod vita_ffi {
    use libc::{c_char, c_int, c_uint, c_ushort, c_void, size_t};

    pub const SCE_SYSMODULE_NET: c_ushort = 0x0001;
    pub const SCE_SYSMODULE_APPUTIL: c_ushort = 0x000F;
    pub const SCE_SYSTEM_PARAM_ID_LANG: c_uint = 1;
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_RW: u32 = 0x0C20_D060;
    /// Prevents automatic standby only; the screen may still dim and switch off.
    pub const SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND: c_int = 1;

    #[repr(C)]
    pub struct SceNetInitParam {
        pub memory: *mut c_void,
        pub size: c_int,
        pub flags: c_int,
    }

    #[repr(C)]
    pub struct SceAppUtilInitParam {
        pub work_buf_size: size_t,
        pub reserved: [u8; 60],
    }

    #[repr(C)]
    pub struct SceAppUtilBootParam {
        pub attr: c_uint,
        pub app_version: c_uint,
        pub reserved: [u8; 32],
    }

    #[link(name = "SceSysmodule_stub")]
    extern "C" {
        pub fn sceSysmoduleLoadModule(id: c_ushort) -> c_int;
    }

    #[link(name = "SceAppUtil_stub")]
    extern "C" {
        pub fn sceAppUtilInit(
            init_param: *mut SceAppUtilInitParam,
            boot_param: *mut SceAppUtilBootParam,
        ) -> c_int;
        pub fn sceAppUtilShutdown() -> c_int;
        pub fn sceAppUtilSystemParamGetInt(param_id: c_uint, value: *mut c_int) -> c_int;
    }

    #[link(name = "SceLibKernel_stub")]
    extern "C" {
        pub fn sceKernelGetRandomNumber(output: *mut c_void, size: u32) -> c_int;
        pub fn sceKernelPowerTick(tick_type: c_int) -> c_int;
    }

    #[link(name = "ScePower_stub")]
    extern "C" {
        /// Returns 1 when the console runs on external power (AC adapter / USB charging).
        pub fn scePowerIsPowerOnline() -> c_int;
        pub fn scePowerGetArmClockFrequency() -> c_int;
        pub fn scePowerSetArmClockFrequency(freq: c_int) -> c_int;
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
        pub fn sceNetCtlInetGetState(state: *mut c_int) -> c_int;
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

/// Reports the console's Wi-Fi link (`sceNetCtlInetGetState`). Needs `init_vita_network` first.
/// A userland application cannot switch Wi-Fi on: when it is off, the user is asked to do it
/// (`application::WifiPrompt`).
#[cfg(target_os = "vita")]
pub fn wifi_link() -> WifiLink {
    const SCE_NETCTL_STATE_DISCONNECTED: libc::c_int = 0;
    const SCE_NETCTL_STATE_CONNECTING: libc::c_int = 1;
    const SCE_NETCTL_STATE_FINALIZING: libc::c_int = 2;
    const SCE_NETCTL_STATE_CONNECTED: libc::c_int = 3;

    let mut state: libc::c_int = -1;
    let ret = unsafe { vita_ffi::sceNetCtlInetGetState(&mut state) };
    if ret < 0 {
        return WifiLink::Unknown;
    }
    match state {
        SCE_NETCTL_STATE_DISCONNECTED => WifiLink::Disconnected,
        SCE_NETCTL_STATE_CONNECTING | SCE_NETCTL_STATE_FINALIZING => WifiLink::Connecting,
        SCE_NETCTL_STATE_CONNECTED => WifiLink::Connected,
        _ => WifiLink::Unknown,
    }
}

/// Host builds use the host's network.
#[cfg(not(target_os = "vita"))]
pub fn wifi_link() -> WifiLink {
    WifiLink::Connected
}

/// Keeps the PS Vita out of automatic standby (see `application::power_policy` for when).
/// Standby suspends Wi-Fi and drops every BEP connection; the screen can still dim / switch off.
/// Must be called periodically (the standby timer is reset by each call).
#[cfg(target_os = "vita")]
pub fn keep_system_awake() {
    unsafe {
        vita_ffi::sceKernelPowerTick(vita_ffi::SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND);
    }
}

#[cfg(not(target_os = "vita"))]
pub fn keep_system_awake() {}

/// Returns `true` when the console is on external power (charging).
#[cfg(target_os = "vita")]
pub fn is_on_external_power() -> bool {
    unsafe { vita_ffi::scePowerIsPowerOnline() == 1 }
}

#[cfg(not(target_os = "vita"))]
pub fn is_on_external_power() -> bool {
    false
}

/// Sets the ARM CPU clock (MHz) if it differs from the current one (see
/// `application::power_policy::cpu_clock`). Safe to call periodically: the system may reset
/// the clock after standby.
#[cfg(target_os = "vita")]
pub fn set_cpu_clock_mhz(mhz: u32) {
    unsafe {
        if vita_ffi::scePowerGetArmClockFrequency() != mhz as libc::c_int {
            let _ = vita_ffi::scePowerSetArmClockFrequency(mhz as libc::c_int);
        }
    }
}

#[cfg(not(target_os = "vita"))]
pub fn set_cpu_clock_mhz(_mhz: u32) {}

/// Detects the system language from VitaSDK `SceAppUtil` (on PS Vita) or environment variables (on host).
#[cfg(target_os = "vita")]
pub fn detect_system_language() -> Language {
    use vita_ffi::{
        sceAppUtilInit, sceAppUtilShutdown, sceAppUtilSystemParamGetInt, sceSysmoduleLoadModule,
        SceAppUtilBootParam, SceAppUtilInitParam, SCE_SYSMODULE_APPUTIL, SCE_SYSTEM_PARAM_ID_LANG,
    };

    unsafe {
        let _ = sceSysmoduleLoadModule(SCE_SYSMODULE_APPUTIL);
        let mut init_param = SceAppUtilInitParam {
            work_buf_size: 0,
            reserved: [0u8; 60],
        };
        let mut boot_param = SceAppUtilBootParam {
            attr: 0,
            app_version: 0,
            reserved: [0u8; 32],
        };
        let ret = sceAppUtilInit(&mut init_param, &mut boot_param);
        if ret < 0 {
            return Language::English;
        }

        let mut lang_val: libc::c_int = 0;
        let get_ret = sceAppUtilSystemParamGetInt(SCE_SYSTEM_PARAM_ID_LANG, &mut lang_val);
        let _ = sceAppUtilShutdown();

        if get_ret < 0 {
            return Language::English;
        }

        match lang_val {
            1 | 18 => Language::English,
            2 => Language::French,
            3 => Language::Spanish,
            4 => Language::German,
            5 => Language::Italian,
            _ => Language::English,
        }
    }
}

#[cfg(not(target_os = "vita"))]
pub fn detect_system_language() -> Language {
    if let Ok(lang_var) = std::env::var("LANG").or_else(|_| std::env::var("LC_ALL")) {
        let prefix = &lang_var[..lang_var.len().min(2)];
        match prefix {
            "fr" | "FR" => Language::French,
            "es" | "ES" => Language::Spanish,
            "de" | "DE" => Language::German,
            "it" | "IT" => Language::Italian,
            _ => Language::English,
        }
    } else {
        Language::English
    }
}
