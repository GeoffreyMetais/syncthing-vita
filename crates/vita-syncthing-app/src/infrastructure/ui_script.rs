//! Opt-in (`--features ui-script`) scripted input replay for emulator smoke tests.
//! Reads `ux0:data/syncthing/ui_script.txt`: one step per line —
//! `wait <frames>`, `press <BUTTON>`, `hold <BUTTON> <frames>` (pressed, then held down),
//! `tap <x> <y>`, `drag <x> <y0> <y1> <frames>` (finger moved from `y0` to `y1`, then lifted),
//! `ime <text>` (submits text to the IME), `shot <name>` (dumps the next presented frame to `ux0:data/syncthing/shots/<name>.rgba`).

use std::sync::Mutex;

use crate::infrastructure::input::*;

static PENDING_SHOT: Mutex<Option<String>> = Mutex::new(None);

/// True if a `shot` step is waiting for the next presented frame (forces a redraw).
pub fn shot_pending() -> bool {
    PENDING_SHOT.lock().map(|p| p.is_some()).unwrap_or(false)
}

/// Writes `pixels` (960x544 RGBA8888) to disk if a `shot` step requested it.
pub fn dump_if_requested(pixels: &[u32]) {
    let Some(name) = PENDING_SHOT.lock().ok().and_then(|mut p| p.take()) else {
        return;
    };
    let _ = std::fs::create_dir_all("ux0:data/syncthing/shots");
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
    let _ = std::fs::write(format!("ux0:data/syncthing/shots/{}.rgba", name), bytes);
}

const SCRIPT_PATH: &str = "ux0:data/syncthing/ui_script.txt";

/// Scripted finger movement in progress (`drag` step).
struct ScriptDrag {
    x: usize,
    y0: i32,
    y1: i32,
    frames: u32,
    frame: u32,
}

pub struct UiScript {
    steps: Vec<String>,
    pos: usize,
    wait: u32,
    /// Button held down, and for how many more frames (`hold` step).
    hold: Option<(u32, u32)>,
    drag: Option<ScriptDrag>,
}

impl UiScript {
    pub fn load() -> Self {
        let steps = std::fs::read_to_string(SCRIPT_PATH)
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        Self {
            steps,
            pos: 0,
            wait: 0,
            hold: None,
            drag: None,
        }
    }

    /// Injects the next scripted step into `frame` (one step per frame).
    pub fn apply(&mut self, frame: &mut InputFrame) {
        if let Some((button, left)) = self.hold {
            frame.held |= button;
            frame.any_activity = true;
            self.hold = (left > 1).then_some((button, left - 1));
            return;
        }
        if let Some(drag) = self.drag.as_mut() {
            frame.any_activity = true;
            if drag.frame > drag.frames {
                // Finger lifted.
                self.drag = None;
                return;
            }
            let y = drag.y0 + (drag.y1 - drag.y0) * drag.frame as i32 / drag.frames.max(1) as i32;
            let pos = (drag.x, y.clamp(0, 543) as usize);
            if drag.frame == 0 {
                frame.touch_down = Some(pos);
            }
            frame.touch_pos = Some(pos);
            drag.frame += 1;
            return;
        }
        if self.wait > 0 {
            self.wait -= 1;
            frame.any_activity = true;
            return;
        }
        let Some(step) = self.steps.get(self.pos).cloned() else {
            return;
        };
        self.pos += 1;
        frame.any_activity = true;
        let mut parts = step.split_whitespace();
        match parts.next() {
            Some("wait") => self.wait = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1),
            Some("press") => frame.pressed |= button(parts.next().unwrap_or("")),
            Some("hold") => {
                let b = button(parts.next().unwrap_or(""));
                let frames: u32 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
                frame.pressed |= b;
                frame.held |= b;
                self.hold = (frames > 1).then_some((b, frames - 1));
            }
            Some("drag") => {
                let mut num = || {
                    parts
                        .next()
                        .and_then(|n| n.parse::<i32>().ok())
                        .unwrap_or(0)
                };
                let (x, y0, y1, frames) = (num(), num(), num(), num());
                self.drag = Some(ScriptDrag {
                    x: x.clamp(0, 959) as usize,
                    y0,
                    y1,
                    frames: frames.max(1) as u32,
                    frame: 0,
                });
            }
            Some("tap") => {
                let x = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
                let y = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
                frame.touch_tap = Some((x, y));
            }
            Some("shot") => {
                if let Ok(mut p) = PENDING_SHOT.lock() {
                    *p = parts.next().map(str::to_owned);
                }
            }
            Some("ime") => {
                frame.ime_text = Some(parts.collect::<Vec<_>>().join(" "));
                frame.ime_submitted = true;
            }
            _ => {}
        }
    }
}

fn button(name: &str) -> u32 {
    match name {
        "CROSS" => BTN_CROSS,
        "CIRCLE" => BTN_CIRCLE,
        "SQUARE" => BTN_SQUARE,
        "TRIANGLE" => BTN_TRIANGLE,
        "UP" => BTN_UP,
        "DOWN" => BTN_DOWN,
        "LEFT" => BTN_LEFT,
        "RIGHT" => BTN_RIGHT,
        "L" => BTN_L,
        "R" => BTN_R,
        "START" => BTN_START,
        "SELECT" => BTN_SELECT,
        _ => 0,
    }
}
