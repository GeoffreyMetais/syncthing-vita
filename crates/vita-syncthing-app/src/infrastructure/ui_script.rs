//! Opt-in (`--features ui-script`) scripted input replay for emulator smoke tests.
//! Reads `ux0:data/syncthing/ui_script.txt`: one step per line —
//! `wait <frames>`, `press <BUTTON>`, `tap <x> <y>`, `ime <text>` (submits text to the IME),
//! `shot <name>` (dumps the next presented frame to `ux0:data/syncthing/shots/<name>.rgba`).

use std::sync::Mutex;

use crate::infrastructure::input::*;

static PENDING_SHOT: Mutex<Option<String>> = Mutex::new(None);

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

pub struct UiScript {
    steps: Vec<String>,
    pos: usize,
    wait: u32,
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
        }
    }

    /// Injects the next scripted step into `frame` (one step per frame).
    pub fn apply(&mut self, frame: &mut InputFrame) {
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
