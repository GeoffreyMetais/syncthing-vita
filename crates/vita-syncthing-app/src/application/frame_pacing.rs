//! Dynamic frame rate: decides on each display VBlank whether a new frame must be rendered.
//!
//! The UI is drawn in software (960x544 RGBA8888) and copied to the hardware framebuffer, which
//! costs CPU time and battery. Most of the time the screen is static, so frames are only rendered
//! when something changed, and continuous animations get a reduced fixed rate:
//!
//! - **User input** (button press, tap, system keyboard text, dialogs): rendered on the next VBlank,
//!   so navigation stays as responsive as a 60 FPS loop.
//! - **Animations** (OLED screensaver, QR camera viewfinder): `ANIMATION_INTERVAL` (30 FPS).
//! - **Background updates** (sync progress, logs, discovered peers): coalesced to
//!   `BACKGROUND_INTERVAL` (10 FPS) so a fast transfer does not force full-rate redraws.
//! - **Static screen**: one safety refresh every `IDLE_REFRESH_INTERVAL` (1 FPS).
//!
//! The main loop still wakes up on every VBlank (60 Hz) to poll input, which is cheap compared
//! to rendering a frame.

pub use crate::domain::defaults::{
    ANIMATION_INTERVAL, BACKGROUND_INTERVAL, IDLE_REFRESH_INTERVAL, VBLANK_HZ,
};

/// What happened since the previous VBlank.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameSignals {
    /// The user interacted with the UI (or a dialog opened / a toast expired): redraw now.
    pub user_change: bool,
    /// Background state changed (worker messages, sync status): redraw soon, rate-limited.
    pub background_change: bool,
    /// A continuous animation is on screen (screensaver, camera preview).
    pub animating: bool,
}

/// Frame rate governor, ticked once per display VBlank by the main loop.
#[derive(Debug, Clone)]
pub struct FramePacer {
    /// VBlanks elapsed since the last rendered frame.
    since_render: u32,
    /// A background change is waiting for its rate-limited redraw.
    pending_background: bool,
}

impl Default for FramePacer {
    fn default() -> Self {
        Self::new()
    }
}

impl FramePacer {
    /// Creates a pacer that renders on the first tick.
    pub fn new() -> Self {
        Self {
            since_render: IDLE_REFRESH_INTERVAL,
            pending_background: false,
        }
    }

    /// Advances by one VBlank. Returns `Some(elapsed)` if a frame must be rendered now, where
    /// `elapsed` is the number of VBlanks since the previous frame (to scale animations).
    pub fn tick(&mut self, signals: FrameSignals) -> Option<u32> {
        self.since_render = self.since_render.saturating_add(1);
        self.pending_background |= signals.background_change;

        let render = signals.user_change
            || (signals.animating && self.since_render >= ANIMATION_INTERVAL)
            || (self.pending_background && self.since_render >= BACKGROUND_INTERVAL)
            || self.since_render >= IDLE_REFRESH_INTERVAL;
        if !render {
            return None;
        }
        let elapsed = self.since_render;
        self.since_render = 0;
        self.pending_background = false;
        Some(elapsed)
    }
}
