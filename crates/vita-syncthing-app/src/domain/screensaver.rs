//! Motion of the OLED screensaver widget (`ScreensaverMotion`): it bounces around the screen
//! so no pixel stays lit at the same place.

/// Margin kept between the widget and the screen edges.
const EDGE_MARGIN: f32 = 12.0;

/// Top-left position of the screensaver widget and its speed, in pixels per display VBlank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreensaverMotion {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
}

impl Default for ScreensaverMotion {
    fn default() -> Self {
        Self {
            x: 180.0,
            y: 160.0,
            vx: 0.65,
            vy: 0.45,
        }
    }
}

impl ScreensaverMotion {
    /// Advances the widget (of `box_w` x `box_h`) within the screen by `frames` display
    /// VBlanks: the speed is the same whatever the rendering frame rate.
    pub fn step(&mut self, screen_w: f32, screen_h: f32, box_w: f32, box_h: f32, frames: u32) {
        let max_x = screen_w - box_w - EDGE_MARGIN;
        let max_y = screen_h - box_h - EDGE_MARGIN;
        for _ in 0..frames {
            self.x += self.vx;
            self.y += self.vy;
            bounce(&mut self.x, &mut self.vx, max_x);
            bounce(&mut self.y, &mut self.vy, max_y);
        }
    }
}

/// Keeps `pos` within `EDGE_MARGIN..=max`, reversing `speed` when an edge is reached.
fn bounce(pos: &mut f32, speed: &mut f32, max: f32) {
    if *pos < EDGE_MARGIN {
        *pos = EDGE_MARGIN;
        *speed = -*speed;
    } else if *pos > max {
        *pos = max;
        *speed = -*speed;
    }
}
