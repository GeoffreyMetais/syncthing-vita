---
name: vitasend-ui-design
description: >-
  Use this skill when adding or modifying UI screens, modal dialogs, vector icons,
  TrueType text layouts, or gamepad/touchscreen input handlers in
  crates/vita-syncthing-app/src/{gfx.rs,display.rs,main.rs}.
---

# VitaSend Native PS Vita UI & Input Skill

`vita-syncthing-app` renders a native `960x544` RGBA8888 interface @ 60 FPS modeled after **VitaSend** (`gfx.rs`, `display.rs`, `input.rs`, `main.rs`).

## 1. Screen Coordinate Grid (`960x544`)

| Region | Bounds (`x`, `y`) | Constants (`display.rs`) |
| :--- | :--- | :--- |
| **Left Sidebar** | `x = 0..212`, `y = 0..500` | `SIDEBAR_W = 212` |
| **Page Title Bar** | `x = 236..936`, `y = 24..68` | `draw_page_title(title, right)` |
| **Main Content Area** | `x = 236..936`, `y = 86..488` | `CONTENT_X = 236`, `CONTENT_R = 936`, `CONTENT_W = 700`, `CONTENT_Y = 86`, `CONTENT_B = 488` |
| **Bottom Hint Bar** | `x = 0..960`, `y = 500..544` | `HINT_Y = 500`, `draw_hints(&[...])` |

## 2. Reusable UI Primitives (`display.rs` & `gfx.rs`)

- **Cards**: `self.draw_card(x, y, w, h, focused)` — `14px` rounded corners, uses `self.th.card` / `self.th.card_hi` and `self.th.accent` border when focused.
- **Status Chips**: `self.draw_chip(right_x, cy, text, col) -> width` — translucent `24px`-high pill aligned from `right_x`.
- **Action Pill Buttons**:
  - Measure width first: `let w = self.action_width(PadButton::Cross, "Label");`
  - Render: `self.draw_action(x, cy, PadButton::Cross, "Label");`
- **Circular Checkboxes**: `self.draw_row_tick(cx, cy, checked)` — filled accent circle with `Icon::Check` when `true`, ring when `false`.
- **Modal Dialogs (`ModalDialog`)**:
  1. Call `self.gfx.dim(180)` to darken the underlying frame.
  2. Center the modal box: `dx = (GFX_W - w) / 2`, `dy = (HINT_Y - h) / 2`.
  3. Draw drop shadow + card:
     ```rust
     self.gfx.shadow(dx, dy, w, h, 18);
     self.gfx.rrect(dx, dy, w, h, 18, self.th.card);
     self.gfx.rrect_outline(dx, dy, w, h, 18, 1, self.th.border);
     ```
  4. Render bottom hints via `self.draw_hints(&[...])` at the end of the modal render method so the bottom bar reflects the modal's active buttons.

## 3. Dual Input Checklist (`SceCtrl` Gamepad + `SceTouch` Touchscreen)

Every interactive element **must** support both gamepad buttons and front touchscreen taps (`main.rs`):
1. **Gamepad (`input.pressed`)**:
   - Wire `BTN_CROSS`, `BTN_CIRCLE`, `BTN_SQUARE`, `BTN_TRIANGLE`, `BTN_UP`, `BTN_DOWN`, `BTN_L`, `BTN_R`, `BTN_SELECT`, `BTN_START` in `handle_user_input` (or `handle_modal_input` if a modal is active).
2. **Front Touchscreen (`input.touch_tap: Option<(usize, usize)>`)**:
   - Touch coordinates in `InputFrame` are already scaled from `1920x1088` to `0..960`, `0..544`.
   - Add matching `(tx, ty)` hitbox checks in:
     - `handle_modal_input` (when `ui.modal != ModalDialog::None`)
     - Bottom Hint Bar (`ty >= 500`)
     - Main Content Area (`tx >= 236 && ty < 500`)
3. **Theme Compatibility**:
   - Always use `self.th.*` (`bg`, `sidebar`, `card`, `card_hi`, `border`, `text`, `dim`, `faint`, `accent`, `accent_hi`, `accent_bg`, `on_accent`) instead of hardcoded backgrounds so both **Dark** and **OLED (`#000000`)** themes render properly.
