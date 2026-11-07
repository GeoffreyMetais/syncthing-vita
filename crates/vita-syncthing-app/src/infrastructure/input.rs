//! Unified PS Vita input hardware driver:
//! - Gamepad & analog sticks (`SceCtrl`: D-Pad, Cross, Circle, Square, Triangle, L, R, Select, Start)
//! - Front touchscreen (`SceTouch`: coordinate scaling `1920x1088` -> `960x544` and tap/press detection)

#[allow(dead_code)]
pub const BTN_SELECT: u32 = 0x0000_0001;
#[allow(dead_code)]
pub const BTN_START: u32 = 0x0000_0008;
#[allow(dead_code)]
pub const BTN_UP: u32 = 0x0000_0010;
#[allow(dead_code)]
pub const BTN_RIGHT: u32 = 0x0000_0020;
#[allow(dead_code)]
pub const BTN_DOWN: u32 = 0x0000_0040;
#[allow(dead_code)]
pub const BTN_LEFT: u32 = 0x0000_0080;
#[allow(dead_code)]
pub const BTN_L: u32 = 0x0000_0100;
#[allow(dead_code)]
pub const BTN_R: u32 = 0x0000_0200;
#[allow(dead_code)]
pub const BTN_TRIANGLE: u32 = 0x0000_1000;
#[allow(dead_code)]
pub const BTN_CIRCLE: u32 = 0x0000_2000;
#[allow(dead_code)]
pub const BTN_CROSS: u32 = 0x0000_4000;
#[allow(dead_code)]
pub const BTN_SQUARE: u32 = 0x0000_8000;

#[cfg(target_os = "vita")]
mod input_ffi {
    use libc::{c_int, c_uint};

    pub const SCE_CTRL_MODE_ANALOG: c_int = 1;
    pub const SCE_TOUCH_PORT_FRONT: c_uint = 0;
    pub const SCE_TOUCH_SAMPLING_STATE_START: c_uint = 1;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SceCtrlData {
        pub time_stamp: u64,
        pub buttons: u32,
        pub lx: u8,
        pub ly: u8,
        pub rx: u8,
        pub ry: u8,
        pub up: u8,
        pub right: u8,
        pub down: u8,
        pub left: u8,
        pub lt: u8,
        pub rt: u8,
        pub l1: u8,
        pub r1: u8,
        pub triangle: u8,
        pub circle: u8,
        pub cross: u8,
        pub square: u8,
        pub reserved: [u8; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct SceTouchReport {
        pub id: u8,
        pub force: u8,
        pub x: i16,
        pub y: i16,
        pub reserved: [u8; 8],
        pub info: u16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SceTouchData {
        pub time_stamp: u64,
        pub status: u32,
        pub report_num: u32,
        pub report: [SceTouchReport; 8],
    }

    #[link(name = "SceCtrl_stub")]
    extern "C" {
        pub fn sceCtrlSetSamplingMode(mode: c_int) -> c_int;
        pub fn sceCtrlPeekBufferPositive(
            port: c_int,
            pad_data: *mut SceCtrlData,
            count: c_int,
        ) -> c_int;
    }

    #[link(name = "SceTouch_stub")]
    extern "C" {
        pub fn sceTouchSetSamplingState(port: c_uint, state: c_uint) -> c_int;
        pub fn sceTouchPeek(port: c_uint, p_data: *mut SceTouchData, n_bufs: c_uint) -> c_int;
    }
}

/// Input state sampled on each frame (60 Hz).
#[derive(Debug, Clone, Copy, Default)]
pub struct InputFrame {
    /// Buttons pressed on this frame (rising edge).
    pub pressed: u32,
    /// Screen coordinates (`0..960`, `0..544`) of a new touch tap on this frame.
    pub touch_tap: Option<(usize, usize)>,
    /// Screen coordinates (`0..960`, `0..544`) of active touch contact.
    #[allow(dead_code)]
    pub touch_pos: Option<(usize, usize)>,
    /// True if any activity (button, stick, or touch) occurred on this frame.
    pub any_activity: bool,
}

pub struct VitaInput {
    prev_buttons: u32,
    prev_touch_down: bool,
}

impl VitaInput {
    pub fn new() -> Self {
        #[cfg(target_os = "vita")]
        unsafe {
            let _ = input_ffi::sceCtrlSetSamplingMode(input_ffi::SCE_CTRL_MODE_ANALOG);
            let _ = input_ffi::sceTouchSetSamplingState(
                input_ffi::SCE_TOUCH_PORT_FRONT,
                input_ffi::SCE_TOUCH_SAMPLING_STATE_START,
            );
        }

        Self {
            prev_buttons: 0,
            prev_touch_down: false,
        }
    }

    pub fn poll(&mut self) -> InputFrame {
        #[cfg(target_os = "vita")]
        unsafe {
            use input_ffi::*;

            let mut pad: SceCtrlData = std::mem::zeroed();
            let mut buttons = 0u32;
            if sceCtrlPeekBufferPositive(0, &mut pad, 1) >= 0 {
                buttons = pad.buttons;
                // Also convert definite left analog stick deflection into D-Pad events
                if pad.ly < 50 {
                    buttons |= BTN_UP;
                } else if pad.ly > 205 {
                    buttons |= BTN_DOWN;
                }
                if pad.lx < 50 {
                    buttons |= BTN_LEFT;
                } else if pad.lx > 205 {
                    buttons |= BTN_RIGHT;
                }
            }

            let pressed = buttons & !self.prev_buttons;
            self.prev_buttons = buttons;

            let mut touch: SceTouchData = std::mem::zeroed();
            let mut touch_pos = None;
            let mut touch_tap = None;

            if sceTouchPeek(SCE_TOUCH_PORT_FRONT, &mut touch, 1) >= 0 && touch.report_num > 0 {
                let rep = touch.report[0];
                let sx = ((rep.x.max(0) as usize) / 2).min(959);
                let sy = ((rep.y.max(0) as usize) / 2).min(543);
                touch_pos = Some((sx, sy));
                if !self.prev_touch_down {
                    touch_tap = Some((sx, sy));
                }
                self.prev_touch_down = true;
            } else {
                self.prev_touch_down = false;
            }

            let any_activity = pressed != 0 || touch_pos.is_some();

            InputFrame {
                pressed,
                touch_tap,
                touch_pos,
                any_activity,
            }
        }

        #[cfg(not(target_os = "vita"))]
        {
            let _ = (self.prev_buttons, self.prev_touch_down);
            InputFrame::default()
        }
    }
}
