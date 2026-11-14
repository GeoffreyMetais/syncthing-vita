//! Input gestures recognized over consecutive VBlanks (`InputGestures`), applied by the main
//! loop to each polled `InputFrame` before it is handled:
//! - **Key repeat**: a held `Up` / `Down` repeats after `KEY_REPEAT_DELAY`, every
//!   `KEY_REPEAT_INTERVAL` (lists scroll while the button is held).
//! - **Touch tap / drag**: a finger lifted without moving more than `TOUCH_DRAG_THRESHOLD` is
//!   a tap (`InputFrame::touch_tap`, at the touch-down position); beyond that the touch is a
//!   drag (`InputFrame::touch_drag`) that scrolls the list under it, and no tap is emitted.

use crate::domain::defaults::{KEY_REPEAT_DELAY, KEY_REPEAT_INTERVAL, TOUCH_DRAG_THRESHOLD};
use crate::infrastructure::{InputFrame, TouchDrag, BTN_DOWN, BTN_UP};

/// Buttons that repeat while held.
const REPEATING_BUTTONS: u32 = BTN_UP | BTN_DOWN;

/// Gesture state carried from one VBlank to the next (one `apply` per polled frame).
#[derive(Debug, Default)]
pub struct InputGestures {
    /// Repeating button currently held, and for how many VBlanks.
    held_button: u32,
    held_vblanks: u32,
    /// Touch in progress: where it started, and whether it became a drag.
    touch_start: Option<(usize, usize)>,
    dragging: bool,
}

impl InputGestures {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the key repeats, taps and drags recognized on this frame to `frame`.
    pub fn apply(&mut self, frame: &mut InputFrame) {
        self.repeat_keys(frame);
        self.track_touch(frame);
    }

    fn repeat_keys(&mut self, frame: &mut InputFrame) {
        let newly = frame.pressed & REPEATING_BUTTONS;
        let held = frame.held & REPEATING_BUTTONS;
        if newly != 0 {
            // A new press restarts the delay (the latest direction wins).
            self.held_button = newly & newly.wrapping_neg();
            self.held_vblanks = 0;
            return;
        }
        if self.held_button == 0 || held & self.held_button == 0 {
            self.held_button = 0;
            self.held_vblanks = 0;
            return;
        }
        self.held_vblanks += 1;
        if self.held_vblanks >= KEY_REPEAT_DELAY
            && (self.held_vblanks - KEY_REPEAT_DELAY).is_multiple_of(KEY_REPEAT_INTERVAL)
        {
            frame.pressed |= self.held_button;
        }
    }

    fn track_touch(&mut self, frame: &mut InputFrame) {
        if let Some(down) = frame.touch_down {
            self.touch_start = Some(down);
            self.dragging = false;
        }
        let Some(start) = self.touch_start else {
            return;
        };
        match frame.touch_pos {
            Some(pos) => {
                let dx = pos.0 as i32 - start.0 as i32;
                let dy = pos.1 as i32 - start.1 as i32;
                let started = !self.dragging
                    && (dx.abs() > TOUCH_DRAG_THRESHOLD || dy.abs() > TOUCH_DRAG_THRESHOLD);
                self.dragging |= started;
                if self.dragging {
                    frame.touch_drag = Some(TouchDrag {
                        start,
                        total_dy: dy,
                        started,
                    });
                }
            }
            None => {
                // Finger lifted: a touch that never dragged is a tap where it started.
                if !self.dragging && frame.touch_tap.is_none() {
                    frame.touch_tap = Some(start);
                }
                self.touch_start = None;
                self.dragging = false;
            }
        }
    }
}
