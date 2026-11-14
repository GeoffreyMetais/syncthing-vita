---
name: vitasend-ui-design
description: >-
  Use this skill when adding or modifying UI screens, modal dialogs, vector icons,
  TrueType text layouts, or gamepad/touchscreen input handlers in
  crates/vita-syncthing-app/src/{ui/**, application/input_controller.rs}.
---

# VitaSend Native PS Vita UI & Input Skill

`vita-syncthing-app` renders a native `960x544` RGBA8888 interface @ 60 FPS modeled after **VitaSend** following Clean Architecture (`ui/`, `application/input_controller.rs`, `infrastructure/input.rs`, `main.rs`).

## 1. Screen Coordinate Grid (`960x544`)

| Region | Bounds (`x`, `y`) | UI Coordinates (`ui/theme.rs` & `ui/renderer.rs`) |
| :--- | :--- | :--- |
| **Left Sidebar** | `x = 0..212`, `y = 0..500` | `SIDEBAR_W = 212` |
| **Page Title Bar** | `x = 236..936`, `y = 24..68` | `draw_page_title(title, right)` |
| **Main Content Area** | `x = 236..936`, `y = 86..488` | `CONTENT_X = 236`, `CONTENT_R = 936`, `CONTENT_W = 700`, `CONTENT_Y = 86`, `CONTENT_B = 488` |
| **Bottom Hint Bar** | `x = 0..960`, `y = 500..544` | `HINT_Y = 500`, `draw_hints(&[...])` |

## 2. Reusable UI Primitives (`ui/widgets.rs` & `ui/gfx.rs`)

- **Cards**: `draw_card(gfx, th, x, y, w, h, focused)` — `14px` rounded corners, uses `th.card` / `th.card_hi` and `th.accent` border when focused.
- **Status Chips**: `draw_chip(gfx, th, right_x, cy, text, col) -> width` — translucent `24px`-high pill aligned from `right_x`.
- **Action Pill Buttons**:
  - Measure width first: `let w = action_width(gfx, PadButton::Cross, "Label");`
  - Render: `draw_action(gfx, th, x, cy, PadButton::Cross, "Label");`
- **Circular Checkboxes**: `draw_row_tick(gfx, th, cx, cy, checked)` — filled accent circle with `Icon::Check` when `true`, ring when `false`.
- **Modal Dialogs (`ui/modals/*`)**:
  1. Call `gfx.dim(180)` to darken the underlying frame.
  2. Center the modal box: `dx = (GFX_W - w) / 2`, `dy = (HINT_Y - h) / 2`.
  3. Draw drop shadow + card:
     ```rust
     gfx.shadow(dx, dy, w, h, 18);
     gfx.rrect(dx, dy, w, h, 18, th.card);
     gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);
     ```
  4. Render bottom hints via `draw_hints(gfx, th, &[...])` at the end of the modal render method so the bottom bar reflects the modal's active buttons.

## 3. Dual Input Checklist (`SceCtrl` Gamepad + `SceTouch` Touchscreen)

Every interactive element **must** support both gamepad buttons and front touchscreen taps (`application/input_controller.rs`):
1. **Gamepad (`input.pressed`)**:
   - Wire `BTN_CROSS`, `BTN_CIRCLE`, `BTN_SQUARE`, `BTN_TRIANGLE`, `BTN_UP`, `BTN_DOWN`, `BTN_L`, `BTN_R`, `BTN_SELECT`, `BTN_START` in `InputController::handle_input` (or modal routing when `app_state.active_modal.is_some()`).
2. **Front Touchscreen (`input.touch_tap: Option<(usize, usize)>`)**:
   - Touch coordinates in `InputFrame` are already scaled from `1920x1088` to `0..960`, `0..544`.
   - Add matching `(tx, ty)` hitbox checks in:
     - Active modal routing (when `app_state.active_modal.is_some()`)
     - Bottom Hint Bar (`ty >= 500`)
     - Main Content Area (`tx >= 236 && ty < 500`)
3. **Theme Compatibility**:
   - Always use `th.*` (`bg`, `sidebar`, `card`, `card_hi`, `border`, `text`, `dim`, `faint`, `accent`, `accent_hi`, `accent_bg`, `on_accent`) instead of hardcoded backgrounds so both **Dark** and **OLED (`#000000`)** themes render properly.

## 4. Mandatory Emulator Check & Screenshots (every visual change)

A change that alters what is drawn (layout, label, translation, colour, icon, screen, modal) is not done until:
1. **Tested in Vita3K**: a `ui-script` scenario (`wait` / `press` / `tap` / `shot`) reaches every affected state and runs with `just ui-scenario <script> [secs]`. If only `vita3k.log` is produced with `qt.qpa.wayland: There are no outputs`, rerun with `DISPLAY=:0 SDL_VIDEODRIVER=x11 QT_QPA_PLATFORM=xcb ./scripts/vita3k_scenario.sh <script> 40`.
2. **Screenshots taken and reviewed**: open each PNG of `target/ui-shots/`, check the change is visible and nothing is clipped or overlapping, and show them to the user.
3. **Screenshots in the PR description**: push them to the `pr-screenshots` orphan branch under `<branch>/` and embed them in a `## Screenshots` section (`![caption](https://github.com/GeoffreyMetais/syncthing-vita/blob/pr-screenshots/<branch>/<name>.png?raw=true)`). Exact commands: AGENTS.md §6 *Screenshots for Visual Changes*.
