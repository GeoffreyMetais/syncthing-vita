//! Unified PS Vita input hardware driver:
//! - Gamepad & analog sticks (`SceCtrl`: D-Pad, Cross, Circle, Square, Triangle, L, R, Select, Start)
//! - Front touchscreen (`SceTouch`: coordinate scaling `1920x1088` -> `960x544` and tap/press detection)
//! - Native PS Vita System Keyboard (`SceIme`: `sceImeOpen`, `sceImeUpdate`, `sceImeClose` via `SCE_SYSMODULE_IME`)

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
    use libc::{c_int, c_uint, c_ushort, c_void};

    pub const SCE_CTRL_MODE_ANALOG: c_int = 1;
    pub const SCE_TOUCH_PORT_FRONT: c_uint = 0;
    pub const SCE_TOUCH_SAMPLING_STATE_START: c_uint = 1;

    pub const SCE_SYSMODULE_IME: c_ushort = 0x0011;
    pub const PSP2_SDK_VERSION: u32 = 0x0357_0011;
    pub const SCE_IME_WORK_BUFFER_SIZE: usize = 20 * 1024;
    pub const SCE_IME_TYPE_BASIC_LATIN: u32 = 1;
    pub const SCE_IME_OPTION_NO_ASSISTANCE: u32 = 0x04;
    pub const SCE_IME_ENTER_LABEL_GO: u8 = 3;

    pub const SCE_IME_EVENT_UPDATE_TEXT: u32 = 1;
    pub const SCE_IME_EVENT_PRESS_CLOSE: u32 = 4;
    pub const SCE_IME_EVENT_PRESS_ENTER: u32 = 5;

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

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SceImeRect {
        pub x: u32,
        pub y: u32,
        pub width: u32,
        pub height: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SceImeEditText {
        pub preedit_index: u32,
        pub preedit_length: u32,
        pub caret_index: u32,
        pub str_ptr: *mut u16,
        pub edit_index: u32,
        pub edit_length_change: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union SceImeEventParam {
        pub rect: SceImeRect,
        pub text: SceImeEditText,
        pub caret_index: u32,
        pub reserved: [u8; 40],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SceImeEventData {
        pub id: u32,
        pub param: SceImeEventParam,
    }

    const _: () = assert!(std::mem::size_of::<SceImeEventData>() == 0x2C);

    pub type SceImeEventHandler = unsafe extern "C" fn(arg: *mut c_void, e: *const SceImeEventData);

    #[repr(C)]
    pub struct SceImeParam {
        pub sdk_version: u32,
        pub input_method: u32,
        pub supported_languages: u64,
        pub languages_forced: i32,
        pub ime_type: u32,
        pub option: u32,
        pub work: *mut c_void,
        pub arg: *mut c_void,
        pub handler: Option<SceImeEventHandler>,
        pub filter: *const c_void,
        pub initial_text: *mut u16,
        pub max_text_length: u32,
        pub input_text_buffer: *mut u16,
        pub enter_label: u8,
        pub reserved: [u8; 7],
    }

    const _: () = assert!(std::mem::size_of::<SceImeParam>() == 0x40);

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

    #[link(name = "SceSysmodule_stub")]
    extern "C" {
        pub fn sceSysmoduleLoadModule(id: c_ushort) -> c_int;
    }

    #[link(name = "SceIme_stub")]
    extern "C" {
        pub fn sceImeOpen(param: *const SceImeParam) -> c_int;
        pub fn sceImeUpdate() -> c_int;
        pub fn sceImeClose() -> c_int;
    }
}

/// Input state sampled on each frame (60 Hz).
#[derive(Debug, Clone, Default)]
pub struct InputFrame {
    /// Buttons pressed on this frame (rising edge).
    pub pressed: u32,
    /// Screen coordinates (`0..960`, `0..544`) of a new touch tap on this frame.
    pub touch_tap: Option<(usize, usize)>,
    /// Screen coordinates (`0..960`, `0..544`) of active touch contact.
    #[allow(dead_code)]
    pub touch_pos: Option<(usize, usize)>,
    /// Updated text from the native PS Vita System Keyboard (`SceIme`), if active.
    pub ime_text: Option<String>,
    /// True if the user pressed Enter/Confirm on the PS Vita System Keyboard (`SceIme`).
    pub ime_submitted: bool,
    /// True if the user closed/canceled the PS Vita System Keyboard (`SceIme`).
    pub ime_canceled: bool,
    /// True if any activity (button, stick, touch, or IME) occurred on this frame.
    pub any_activity: bool,
}

impl InputFrame {
    /// True if this input can change what is displayed (a press, a tap or system keyboard
    /// events). A held button or a resting finger keeps `any_activity` set but changes nothing.
    pub fn changes_view(&self) -> bool {
        self.pressed != 0
            || self.touch_tap.is_some()
            || self.ime_text.is_some()
            || self.ime_submitted
            || self.ime_canceled
    }
}

#[cfg(target_os = "vita")]
struct ImeRuntimeState {
    work_buf: Vec<u32>,
    initial_utf16: [u16; 128],
    input_utf16: [u16; 128],
    text_updated: bool,
    enter_pressed: bool,
    close_pressed: bool,
}

#[cfg(target_os = "vita")]
impl ImeRuntimeState {
    fn new() -> Self {
        Self {
            work_buf: vec![0u32; input_ffi::SCE_IME_WORK_BUFFER_SIZE / 4],
            initial_utf16: [0u16; 128],
            input_utf16: [0u16; 128],
            text_updated: false,
            enter_pressed: false,
            close_pressed: false,
        }
    }

    fn current_string(&self) -> String {
        let end = self
            .input_utf16
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.input_utf16.len());
        String::from_utf16_lossy(&self.input_utf16[..end])
    }
}

#[cfg(target_os = "vita")]
unsafe extern "C" fn vita_ime_event_handler(
    arg: *mut libc::c_void,
    e: *const input_ffi::SceImeEventData,
) {
    if arg.is_null() || e.is_null() {
        return;
    }
    let st = &mut *(arg as *mut ImeRuntimeState);
    match (*e).id {
        input_ffi::SCE_IME_EVENT_UPDATE_TEXT => {
            st.text_updated = true;
        }
        input_ffi::SCE_IME_EVENT_PRESS_ENTER => {
            st.text_updated = true;
            st.enter_pressed = true;
        }
        input_ffi::SCE_IME_EVENT_PRESS_CLOSE => {
            st.close_pressed = true;
        }
        _ => {}
    }
}

pub struct VitaInput {
    prev_buttons: u32,
    prev_touch_down: bool,
    ime_open: bool,
    #[cfg(target_os = "vita")]
    ime_state: Box<ImeRuntimeState>,
    #[cfg(feature = "ui-script")]
    script: crate::infrastructure::ui_script::UiScript,
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
            let _ = input_ffi::sceSysmoduleLoadModule(input_ffi::SCE_SYSMODULE_IME);
        }

        Self {
            prev_buttons: 0,
            prev_touch_down: false,
            ime_open: false,
            #[cfg(target_os = "vita")]
            ime_state: Box::new(ImeRuntimeState::new()),
            #[cfg(feature = "ui-script")]
            script: crate::infrastructure::ui_script::UiScript::load(),
        }
    }

    /// Opens the native PS Vita System Keyboard (`sceImeOpen`) pre-filled with `initial_text`.
    /// Returns `true` if the system keyboard overlay was opened on screen.
    pub fn open_ime(&mut self, initial_text: &str) -> bool {
        #[cfg(target_os = "vita")]
        unsafe {
            use input_ffi::*;

            let _ = sceSysmoduleLoadModule(SCE_SYSMODULE_IME);
            if self.ime_open {
                let _ = sceImeClose();
                self.ime_open = false;
            }

            self.ime_state.initial_utf16.fill(0);
            self.ime_state.input_utf16.fill(0);
            for (i, u) in initial_text.encode_utf16().take(64).enumerate() {
                self.ime_state.initial_utf16[i] = u;
                self.ime_state.input_utf16[i] = u;
            }
            self.ime_state.text_updated = false;
            self.ime_state.enter_pressed = false;
            self.ime_state.close_pressed = false;
            self.ime_state.work_buf.fill(0);

            let state_ptr: *mut ImeRuntimeState = &mut *self.ime_state;
            let param = SceImeParam {
                sdk_version: PSP2_SDK_VERSION,
                input_method: 0,
                supported_languages: 0,
                languages_forced: 0,
                ime_type: SCE_IME_TYPE_BASIC_LATIN,
                option: SCE_IME_OPTION_NO_ASSISTANCE,
                work: self.ime_state.work_buf.as_mut_ptr() as *mut libc::c_void,
                arg: state_ptr as *mut libc::c_void,
                handler: Some(vita_ime_event_handler),
                filter: std::ptr::null(),
                initial_text: self.ime_state.initial_utf16.as_mut_ptr(),
                max_text_length: 64,
                input_text_buffer: self.ime_state.input_utf16.as_mut_ptr(),
                enter_label: SCE_IME_ENTER_LABEL_GO,
                reserved: [0u8; 7],
            };

            if sceImeOpen(&param) >= 0 {
                self.ime_open = true;
                return true;
            }
            false
        }

        #[cfg(not(target_os = "vita"))]
        {
            let _ = initial_text;
            self.ime_open = false;
            false
        }
    }

    /// Closes the PS Vita System Keyboard if currently open.
    pub fn close_ime(&mut self) {
        #[cfg(target_os = "vita")]
        if self.ime_open {
            unsafe {
                let _ = input_ffi::sceImeClose();
            }
            self.ime_open = false;
        }
        #[cfg(not(target_os = "vita"))]
        {
            self.ime_open = false;
        }
    }

    pub fn is_ime_open(&self) -> bool {
        self.ime_open
    }

    pub fn poll(&mut self) -> InputFrame {
        #[allow(unused_mut)]
        let mut frame = self.poll_hardware();
        #[cfg(feature = "ui-script")]
        self.script.apply(&mut frame);
        frame
    }

    fn poll_hardware(&mut self) -> InputFrame {
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

            // When the native PS Vita System Keyboard (`SceIme`) is active, pump `sceImeUpdate()`
            // and suppress underlying app button/touch presses so the OS keyboard owns input.
            if self.ime_open {
                let update_rc = sceImeUpdate();
                let ime_text = if self.ime_state.text_updated {
                    self.ime_state.text_updated = false;
                    Some(self.ime_state.current_string())
                } else {
                    None
                };
                let ime_submitted = self.ime_state.enter_pressed;
                let ime_canceled = self.ime_state.close_pressed || update_rc < 0;

                if ime_submitted || ime_canceled {
                    let _ = sceImeClose();
                    self.ime_open = false;
                }

                self.prev_buttons = buttons;
                self.prev_touch_down = true;

                return InputFrame {
                    pressed: 0,
                    touch_tap: None,
                    touch_pos: None,
                    ime_text,
                    ime_submitted,
                    ime_canceled,
                    any_activity: true,
                };
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
                ime_text: None,
                ime_submitted: false,
                ime_canceled: false,
                any_activity,
            }
        }

        #[cfg(not(target_os = "vita"))]
        {
            let _ = (self.prev_buttons, self.prev_touch_down, self.ime_open);
            InputFrame::default()
        }
    }
}
