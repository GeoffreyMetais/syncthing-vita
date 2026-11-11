//! PS Vita Hardware Framebuffer driver (`960x544` RGBA8888 via `SceDisplay` + `SceSysmem`).

#[allow(dead_code)]
pub const SCREEN_W: usize = 960;
#[allow(dead_code)]
pub const SCREEN_H: usize = 544;
const FB_ALIGNED_SIZE: usize = 2 * 1024 * 1024;

#[cfg(target_os = "vita")]
mod display_ffi {
    use libc::{c_char, c_int, c_uint, c_void};

    /// Real VitaSDK CDRAM uncached RW memory block type (`psp2common/kernel/sysmem.h`).
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW: c_uint = 0x09408060;
    /// Real VitaSDK physically contiguous uncached main memory block type (`psp2common/kernel/sysmem.h`).
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_MAIN_PHYCONT_NC_RW: c_uint = 0x0D808060;
    /// Fallback uncached main memory block type (`psp2common/kernel/sysmem.h`).
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_RW_UNCACHE: c_uint = 0x0C208060;
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
        pub fn sceDisplaySetFrameBuf(pParam: *const SceDisplayFrameBuf, sync: c_int) -> c_int;
        pub fn sceDisplayWaitVblankStart() -> c_int;
    }
}

/// Initial fill colour for the hardware buffers (matches the dark UI background, `rgb(15, 19, 23)`).
#[cfg(target_os = "vita")]
const FB_CLEAR_COLOR: u32 = 0xFF17_130F;

/// Manages two physical CDRAM/PHYCONT framebuffers on PS Vita (double buffering) and VBlank presentation.
///
/// The frame is always written into the *back* buffer (not visible), then flipped to the display
/// on the next VBlank via `sceDisplaySetFrameBuf(.., SCE_DISPLAY_SETBUF_NEXTFRAME)`. This prevents
/// tearing/flicker caused by copying into the buffer currently being scanned out.
pub struct VitaFramebuffer {
    #[cfg(target_os = "vita")]
    mem_uids: [i32; 2],
    #[cfg(target_os = "vita")]
    fb_ptrs: [*mut u32; 2],
    /// Index of the buffer that will receive the next frame (not currently displayed).
    #[cfg(target_os = "vita")]
    back_idx: usize,
}

impl VitaFramebuffer {
    pub fn new() -> Self {
        #[cfg(target_os = "vita")]
        {
            Self::init_vita_fb()
        }
        #[cfg(not(target_os = "vita"))]
        {
            let _ = FB_ALIGNED_SIZE;
            Self {}
        }
    }

    #[cfg(target_os = "vita")]
    fn alloc_buffer() -> (i32, *mut u32) {
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
                uid = sceKernelAllocMemBlock(
                    b"vita_syncthing_fb\0".as_ptr() as *const libc::c_char,
                    SCE_KERNEL_MEMBLOCK_TYPE_USER_RW_UNCACHE,
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

            // Clear uninitialized memory so no garbage is ever shown on screen.
            std::slice::from_raw_parts_mut(base as *mut u32, SCREEN_W * SCREEN_H)
                .fill(FB_CLEAR_COLOR);

            (uid, base as *mut u32)
        }
    }

    #[cfg(target_os = "vita")]
    fn set_display_buffer(ptr: *mut u32) {
        use display_ffi::*;

        let fb_param = SceDisplayFrameBuf {
            size: std::mem::size_of::<SceDisplayFrameBuf>() as u32,
            base: ptr as *mut libc::c_void,
            pitch: SCREEN_W as u32,
            pixelformat: 0,
            width: SCREEN_W as u32,
            height: SCREEN_H as u32,
        };
        unsafe {
            let _ = sceDisplaySetFrameBuf(&fb_param, SCE_DISPLAY_SETBUF_NEXTFRAME);
        }
    }

    #[cfg(target_os = "vita")]
    fn init_vita_fb() -> Self {
        let (uid0, ptr0) = Self::alloc_buffer();
        let (uid1, mut ptr1) = Self::alloc_buffer();
        // Fall back to single buffering if the second allocation failed.
        if ptr1.is_null() {
            ptr1 = ptr0;
        }

        Self {
            mem_uids: [uid0, uid1],
            fb_ptrs: [ptr0, ptr1],
            back_idx: 0,
        }
    }

    /// Copies the 960x544 RGBA8888 backbuffer into the hidden hardware buffer,
    /// flips it to the display on the next VBlank, and waits for that VBlank.
    pub fn present(&mut self, pixels: &[u32]) {
        #[cfg(feature = "ui-script")]
        crate::infrastructure::ui_script::dump_if_requested(pixels);
        #[cfg(target_os = "vita")]
        {
            let dst = self.fb_ptrs[self.back_idx];
            if !dst.is_null() && pixels.len() >= SCREEN_W * SCREEN_H {
                unsafe {
                    std::ptr::copy_nonoverlapping(pixels.as_ptr(), dst, SCREEN_W * SCREEN_H);
                }
                Self::set_display_buffer(dst);
                unsafe {
                    let _ = display_ffi::sceDisplayWaitVblankStart();
                }
                self.back_idx ^= 1;
            }
        }
        #[cfg(not(target_os = "vita"))]
        let _ = pixels;
    }
}

impl Drop for VitaFramebuffer {
    fn drop(&mut self) {
        #[cfg(target_os = "vita")]
        let _ = self.mem_uids;
    }
}
