//! Rear-camera QR scanner (`SceCamera`): a background thread captures `640x360` frames,
//! publishes a greyscale preview for the viewfinder and decodes QR codes with `QrDecoder`.
//! On hosts (and consoles without a camera, like the PlayStation TV) it reports an error event.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::domain::CameraPreview;

/// Camera capture size (`SCE_CAMERA_RESOLUTION_640_360`).
pub const CAMERA_W: usize = 640;
pub const CAMERA_H: usize = 360;

/// Stack of the capture / decode thread (`quircs` keeps its large buffers on the heap).
const SCAN_THREAD_STACK: usize = 1024 * 1024;

/// Event produced by the scanner thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrScanEvent {
    /// Text payload of a decoded QR code (reported once until a different code is seen).
    Payload(String),
    /// The camera could not be opened or stopped delivering frames.
    CameraError(String),
}

/// Latest frame shared between the scanner thread and the UI thread.
struct SharedFrame {
    seq: u64,
    gray: Vec<u8>,
}

struct Running {
    stop: Arc<AtomicBool>,
    frame: Arc<Mutex<SharedFrame>>,
    events: Receiver<QrScanEvent>,
    handle: Option<JoinHandle<()>>,
    seen_seq: u64,
}

/// Starts / stops the camera scanner thread and relays its frames and events.
pub struct QrScanner {
    running: Option<Running>,
}

impl QrScanner {
    pub fn new() -> Self {
        Self { running: None }
    }

    /// Runs the camera while `active` is true and releases it as soon as it becomes false.
    pub fn set_active(&mut self, active: bool) {
        match (active, self.running.is_some()) {
            (true, false) => self.running = Some(Self::start()),
            (false, true) => self.stop(),
            _ => {}
        }
    }

    /// Copies a new preview frame into `preview` (if any) and returns the next scanner event.
    pub fn poll(&mut self, preview: &mut CameraPreview) -> Option<QrScanEvent> {
        let running = self.running.as_mut()?;
        if let Ok(frame) = running.frame.lock() {
            if frame.seq != running.seen_seq && frame.gray.len() == CAMERA_W * CAMERA_H {
                running.seen_seq = frame.seq;
                preview.width = CAMERA_W;
                preview.height = CAMERA_H;
                preview.gray.clear();
                preview.gray.extend_from_slice(&frame.gray);
            }
        }
        running.events.try_recv().ok()
    }

    fn start() -> Running {
        let stop = Arc::new(AtomicBool::new(false));
        let frame = Arc::new(Mutex::new(SharedFrame {
            seq: 0,
            gray: Vec::new(),
        }));
        let (tx, rx) = mpsc::channel();

        let thread_stop = Arc::clone(&stop);
        let thread_frame = Arc::clone(&frame);
        let error_tx = tx.clone();
        let handle = std::thread::Builder::new()
            .name("qr_scanner".to_owned())
            .stack_size(SCAN_THREAD_STACK)
            .spawn(move || scan_loop(&thread_stop, &thread_frame, &tx))
            .map_err(|e| {
                let _ = error_tx.send(QrScanEvent::CameraError(format!(
                    "Cannot start camera thread: {}",
                    e
                )));
            })
            .ok();

        Running {
            stop,
            frame,
            events: rx,
            handle,
            seen_seq: 0,
        }
    }

    fn stop(&mut self) {
        if let Some(mut running) = self.running.take() {
            running.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = running.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Default for QrScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for QrScanner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Publishes `gray` as the latest preview frame.
#[cfg(target_os = "vita")]
fn publish_frame(shared: &Mutex<SharedFrame>, gray: &[u8]) {
    if let Ok(mut frame) = shared.lock() {
        frame.gray.clear();
        frame.gray.extend_from_slice(gray);
        frame.seq = frame.seq.wrapping_add(1);
    }
}

/// Decodes `gray` and sends a payload event when a new QR code appears in view.
#[cfg(target_os = "vita")]
fn decode_frame(
    decoder: &mut crate::infrastructure::QrDecoder,
    gray: &[u8],
    last_payload: &mut Option<String>,
    events: &Sender<QrScanEvent>,
) {
    let Some(payload) = decoder.decode(gray, CAMERA_W, CAMERA_H).into_iter().next() else {
        return;
    };
    if last_payload.as_deref() != Some(payload.as_str()) {
        *last_payload = Some(payload.clone());
        let _ = events.send(QrScanEvent::Payload(payload));
    }
}

#[cfg(not(target_os = "vita"))]
fn scan_loop(_stop: &AtomicBool, _frame: &Mutex<SharedFrame>, events: &Sender<QrScanEvent>) {
    let _ = events.send(QrScanEvent::CameraError(
        "No camera available on this platform".to_owned(),
    ));
}

#[cfg(target_os = "vita")]
fn scan_loop(stop: &AtomicBool, frame: &Mutex<SharedFrame>, events: &Sender<QrScanEvent>) {
    let mut camera = match vita_camera::RearCamera::open() {
        Ok(camera) => camera,
        Err(e) => {
            let _ = events.send(QrScanEvent::CameraError(e));
            return;
        }
    };
    let mut decoder = crate::infrastructure::QrDecoder::new();
    let mut gray = vec![0u8; CAMERA_W * CAMERA_H];
    let mut last_payload: Option<String> = None;
    let mut read_failures = 0u32;

    while !stop.load(Ordering::SeqCst) {
        if !camera.read_gray(&mut gray) {
            read_failures += 1;
            if read_failures > 30 {
                let _ = events.send(QrScanEvent::CameraError(
                    "Camera stopped delivering frames".to_owned(),
                ));
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
            continue;
        }
        read_failures = 0;
        publish_frame(frame, &gray);
        decode_frame(&mut decoder, &gray, &mut last_payload, events);
    }
}

#[cfg(target_os = "vita")]
mod vita_camera {
    use super::{CAMERA_H, CAMERA_W};
    use libc::{c_char, c_int, c_uint, c_void};
    use std::ptr;

    const SCE_CAMERA_DEVICE_BACK: c_int = 1;
    const SCE_CAMERA_PRIORITY_SHARE: u16 = 0;
    const SCE_CAMERA_FORMAT_ABGR: u16 = 5;
    const SCE_CAMERA_RESOLUTION_640_360: u16 = 8;
    const SCE_CAMERA_FRAMERATE_30_FPS: u16 = 30;
    const SCE_CAMERA_READ_BLOCKING: c_int = 0;

    // The camera writes frames by DMA: use physically contiguous / uncached memory.
    const MEMBLOCK_TYPES: [c_uint; 3] = [
        0x0940_8060, // SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW
        0x0C80_D060, // SCE_KERNEL_MEMBLOCK_TYPE_USER_MAIN_PHYCONT_NC_RW
        0x0C20_8060, // SCE_KERNEL_MEMBLOCK_TYPE_USER_RW_UNCACHE
    ];
    const FRAME_BYTES: usize = CAMERA_W * CAMERA_H * 4;
    /// 1 MiB satisfies the alignment of every memory block type above.
    const BLOCK_SIZE: usize = (FRAME_BYTES + 0xF_FFFF) & !0xF_FFFF;

    #[repr(C)]
    struct SceCameraInfo {
        size: u32,
        priority: u16,
        format: u16,
        resolution: u16,
        framerate: u16,
        width: u16,
        height: u16,
        range: u16,
        pad: u16,
        size_i_base: u32,
        size_u_base: u32,
        size_v_base: u32,
        p_i_base: *mut c_void,
        p_u_base: *mut c_void,
        p_v_base: *mut c_void,
        pitch: u16,
        buffer: u16,
    }
    const _: () = assert!(std::mem::size_of::<SceCameraInfo>() == 0x30);

    #[repr(C)]
    struct SceCameraRead {
        size: u32,
        mode: c_int,
        pad: c_int,
        status: c_int,
        frame: u64,
        timestamp: u64,
        size_i_base: u32,
        size_u_base: u32,
        size_v_base: u32,
        p_i_base: *mut c_void,
        p_u_base: *mut c_void,
        p_v_base: *mut c_void,
    }
    const _: () = assert!(std::mem::size_of::<SceCameraRead>() == 0x38);

    #[link(name = "SceCamera_stub")]
    extern "C" {
        fn sceCameraOpen(devnum: c_int, info: *mut SceCameraInfo) -> c_int;
        fn sceCameraClose(devnum: c_int) -> c_int;
        fn sceCameraStart(devnum: c_int) -> c_int;
        fn sceCameraStop(devnum: c_int) -> c_int;
        fn sceCameraRead(devnum: c_int, read: *mut SceCameraRead) -> c_int;
    }

    #[link(name = "SceSysmem_stub")]
    extern "C" {
        fn sceKernelAllocMemBlock(
            name: *const c_char,
            ty: c_uint,
            size: c_int,
            opt: *const c_void,
        ) -> c_int;
        fn sceKernelGetMemBlockBase(uid: c_int, base: *mut *mut c_void) -> c_int;
        fn sceKernelFreeMemBlock(uid: c_int) -> c_int;
    }

    /// Open and streaming rear camera writing `640x360` ABGR frames into a DMA buffer.
    pub struct RearCamera {
        block_uid: c_int,
        base: *mut u8,
    }

    impl RearCamera {
        pub fn open() -> Result<Self, String> {
            let (block_uid, base) = alloc_frame_buffer()?;
            let mut info = SceCameraInfo {
                size: std::mem::size_of::<SceCameraInfo>() as u32,
                priority: SCE_CAMERA_PRIORITY_SHARE,
                format: SCE_CAMERA_FORMAT_ABGR,
                resolution: SCE_CAMERA_RESOLUTION_640_360,
                framerate: SCE_CAMERA_FRAMERATE_30_FPS,
                width: 0,
                height: 0,
                range: 0,
                pad: 0,
                size_i_base: FRAME_BYTES as u32,
                size_u_base: 0,
                size_v_base: 0,
                p_i_base: base as *mut c_void,
                p_u_base: ptr::null_mut(),
                p_v_base: ptr::null_mut(),
                pitch: 0,
                buffer: 0,
            };
            unsafe {
                let rc = sceCameraOpen(SCE_CAMERA_DEVICE_BACK, &mut info);
                if rc < 0 {
                    let _ = sceKernelFreeMemBlock(block_uid);
                    return Err(format!("Rear camera unavailable (0x{:08X})", rc as u32));
                }
                let rc = sceCameraStart(SCE_CAMERA_DEVICE_BACK);
                if rc < 0 {
                    let _ = sceCameraClose(SCE_CAMERA_DEVICE_BACK);
                    let _ = sceKernelFreeMemBlock(block_uid);
                    return Err(format!("Cannot start rear camera (0x{:08X})", rc as u32));
                }
            }
            Ok(Self { block_uid, base })
        }

        /// Waits for the next frame and converts it to luma into `gray` (`640 * 360` bytes).
        pub fn read_gray(&mut self, gray: &mut [u8]) -> bool {
            let mut read = SceCameraRead {
                size: std::mem::size_of::<SceCameraRead>() as u32,
                mode: SCE_CAMERA_READ_BLOCKING,
                pad: 0,
                status: 0,
                frame: 0,
                timestamp: 0,
                size_i_base: 0,
                size_u_base: 0,
                size_v_base: 0,
                p_i_base: ptr::null_mut(),
                p_u_base: ptr::null_mut(),
                p_v_base: ptr::null_mut(),
            };
            if unsafe { sceCameraRead(SCE_CAMERA_DEVICE_BACK, &mut read) } < 0 {
                return false;
            }
            let pixels = unsafe { std::slice::from_raw_parts(self.base, FRAME_BYTES) };
            for (dst, px) in gray.iter_mut().zip(pixels.chunks_exact(4)) {
                // Bytes are R, G, B, A (`SCE_CAMERA_FORMAT_ABGR`, little-endian)
                let luma = 77 * px[0] as u32 + 150 * px[1] as u32 + 29 * px[2] as u32;
                *dst = (luma >> 8) as u8;
            }
            true
        }
    }

    impl Drop for RearCamera {
        fn drop(&mut self) {
            unsafe {
                let _ = sceCameraStop(SCE_CAMERA_DEVICE_BACK);
                let _ = sceCameraClose(SCE_CAMERA_DEVICE_BACK);
                let _ = sceKernelFreeMemBlock(self.block_uid);
            }
        }
    }

    fn alloc_frame_buffer() -> Result<(c_int, *mut u8), String> {
        for ty in MEMBLOCK_TYPES {
            unsafe {
                let uid = sceKernelAllocMemBlock(
                    b"vita_syncthing_camera\0".as_ptr() as *const c_char,
                    ty,
                    BLOCK_SIZE as c_int,
                    ptr::null(),
                );
                if uid < 0 {
                    continue;
                }
                let mut base: *mut c_void = ptr::null_mut();
                if sceKernelGetMemBlockBase(uid, &mut base) < 0 || base.is_null() {
                    let _ = sceKernelFreeMemBlock(uid);
                    continue;
                }
                return Ok((uid, base as *mut u8));
            }
        }
        Err("Cannot allocate camera frame buffer".to_owned())
    }
}
