//! PS Vita Hardware Framebuffer driver (`960x544` RGBA8888 via `SceDisplay` + `SceSysmem`).

#[allow(dead_code)]
pub const SCREEN_W: usize = 960;
#[allow(dead_code)]
pub const SCREEN_H: usize = 544;
const FB_ALIGNED_SIZE: usize = 2 * 1024 * 1024;

#[cfg(target_os = "vita")]
mod display_ffi {
    use libc::{c_char, c_int, c_uint, c_void};

    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW: c_uint = 0x0C20D060;
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_MAIN_PHYCONT_NC_RW: c_uint = 0x0C20D050;
    pub const SCE_DISPLAY_SETBUF_NEXTFRAME: c_int = 1;

    #[repr(C)]
    pub struct SceDisplayFrameBuf {
        pub size: c_uint,
        pub base: *mut c_void,
        pub pitch: c_uint,
        pub pixelformat: c_uint,
        pub width: c_uint,
        pub height: c_uint,
    }

    #[link(name = "SceSysmem_stub")]
    extern "C" {
        pub fn sceKernelAllocMemBlock(
            name: *const c_char,
            block_type: c_uint,
            size: c_int,
            opt: *const c_void,
        ) -> c_int;
        pub fn sceKernelGetMemBlockBase(uid: c_int, basep: *mut *mut c_void) -> c_int;
    }

    #[link(name = "SceDisplay_stub")]
    extern "C" {
        pub fn sceDisplaySetFrameBuf(
            pParam: *const SceDisplayFrameBuf,
            sync: c_int,
        ) -> c_int;
        pub fn sceDisplayWaitVblankStart() -> c_int;
    }
}

/// Manages the physical CDRAM/PHYCONT framebuffer on PS Vita and VBlank presentation.
pub struct VitaFramebuffer {
    #[cfg(target_os = "vita")]
    mem_uid: i32,
    #[cfg(target_os = "vita")]
    fb_ptr: *mut u32,
}

impl VitaFramebuffer {
    pub fn new() -> Self {
        #[cfg(target_os = "vita")]
        {
            let (mem_uid, fb_ptr) = Self::init_vita_fb();
            Self { mem_uid, fb_ptr }
        }
        #[cfg(not(target_os = "vita"))]
        {
            let _ = FB_ALIGNED_SIZE;
            Self {}
        }
    }

    #[cfg(target_os = "vita")]
    fn init_vita_fb() -> (i32, *mut u32) {
        use display_ffi::*;
        use std::ptr;

        unsafe {
            let mut uid = sceKernelAllocMemBlock(
                b"vita_syncthing_fb\0".as_ptr() as *const libc::c_char,
                SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW,
                FB_ALIGNED_SIZE as i32,
                ptr::null(),
            );
            if uid < 0 {
                uid = sceKernelAllocMemBlock(
                    b"vita_syncthing_fb\0".as_ptr() as *const libc::c_char,
                    SCE_KERNEL_MEMBLOCK_TYPE_USER_MAIN_PHYCONT_NC_RW,
                    FB_ALIGNED_SIZE as i32,
                    ptr::null(),
                );
            }
            if uid < 0 {
                return (-1, ptr::null_mut());
            }

            let mut base: *mut libc::c_void = ptr::null_mut();
            if sceKernelGetMemBlockBase(uid, &mut base) < 0 || base.is_null() {
                return (uid, ptr::null_mut());
            }

            let fb_param = SceDisplayFrameBuf {
                size: std::mem::size_of::<SceDisplayFrameBuf>() as u32,
                base,
                pitch: SCREEN_W as u32,
                pixelformat: 0,
                width: SCREEN_W as u32,
                height: SCREEN_H as u32,
            };
            let _ = sceDisplaySetFrameBuf(&fb_param, SCE_DISPLAY_SETBUF_NEXTFRAME);
            let _ = sceDisplayWaitVblankStart();

            (uid, base as *mut u32)
        }
    }

    /// Copies the 960x544 RGBA8888 backbuffer to CDRAM and waits for VBlank.
    pub fn present(&self, pixels: &[u32]) {
        #[cfg(target_os = "vita")]
        unsafe {
            if !self.fb_ptr.is_null() && pixels.len() >= SCREEN_W * SCREEN_H {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr(),
                    self.fb_ptr,
                    SCREEN_W * SCREEN_H,
                );
                let _ = display_ffi::sceDisplayWaitVblankStart();
            }
        }
        #[cfg(not(target_os = "vita"))]
        let _ = pixels;
    }
}

impl Drop for VitaFramebuffer {
    fn drop(&mut self) {
        #[cfg(target_os = "vita")]
        let _ = self.mem_uid;
    }
}
