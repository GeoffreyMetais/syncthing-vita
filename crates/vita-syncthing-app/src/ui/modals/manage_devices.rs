//! Modal Dialog: Add or remove Syncthing devices for a share (`shares[share_idx]`).

use syncthing_core::DeviceId;

use crate::domain::AppState;
use crate::ui::gfx::{Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::theme::{UiTheme, C_GOOD, C_WARN, HINT_Y};
use crate::ui::widgets::{action_width, draw_action, draw_chip, draw_hints, draw_row_tick};

/// Modal Dialog 2: Add or remove Syncthing devices for a share (`shares[share_idx]`).
pub fn render_manage_devices_modal(
    gfx: &mut Gfx,
    th: &UiTheme,
    state: &AppState,
    share_idx: usize,
    selected_dev_idx: usize,
) {
    gfx.dim(175);
    let w = 680;
    let h = 390;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let share_title = state
        .shares
        .get(share_idx)
        .map(|s| format!("{} ({})", s.label, s.folder_id))
        .unwrap_or_else(|| "Share".to_owned());
    let shared_list = state
        .shares
        .get(share_idx)
        .map(|s| s.shared_devices.as_slice())
        .unwrap_or(&[]);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Desktop,
        (lx + 20) as f32,
        (dy + 40) as f32,
        22.0,
        th.accent_hi,
    );
    gfx.text_fit(
        lx + 52,
        dy + 32,
        w - 110,
        20,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &format!("Devices for {}", share_title),
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "Tick the remote Syncthing devices that should synchronize this folder",
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    if state.devices.is_empty() {
        let (headline, col) = if state.sync_in_progress {
            (
                "Scanning LAN (UDP 21027 + TCP 22000) for Syncthing peers...",
                th.accent_hi,
            )
        } else {
            ("No remote Syncthing devices discovered yet.", C_WARN)
        };
        gfx.text(
            lx,
            dy + 130,
            16,
            FontWeight::Medium,
            col,
            Align::Left,
            headline,
        );
        let sub_hint = if state.sync_in_progress {
            state.status_line.as_str()
        } else {
            "Press Square (+ Add by ID) to enter a Short/Full ID, or Triangle to scan LAN."
        };
        gfx.text(
            lx,
            dy + 160,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            sub_hint,
        );
    } else {
        for (i, dev) in state.devices.iter().take(5).enumerate() {
            let ry = dy + 92 + (i as i32) * 52;
            let cy = ry + 23;
            let is_sel = i == selected_dev_idx;
            let is_checked = shared_list
                .iter()
                .filter_map(|h| DeviceId::from_hex(h))
                .any(|id| id.matches_peer(&dev.device_id));

            if is_sel {
                gfx.rrect(lx, ry, w - 56, 46, 10, th.accent_bg);
                gfx.rrect(lx, ry + 10, 4, 26, 2, th.accent);
            }

            draw_row_tick(gfx, th, lx + 24, cy, is_checked);
            gfx.icon(
                Icon::Laptop,
                (lx + 54) as f32,
                cy as f32,
                22.0,
                if is_checked { th.accent_hi } else { th.dim },
            );

            let badge_txt = if is_checked { "Linked" } else { "Not linked" };
            let badge_col = if is_checked { C_GOOD } else { th.dim };
            draw_chip(gfx, dx + w - 36, cy, badge_txt, badge_col);

            gfx.text(
                lx + 78,
                cy - 7,
                16,
                FontWeight::Bold,
                th.text,
                Align::Left,
                &dev.name,
            );
            let sub = if dev.device_id.is_short_placeholder() {
                format!(
                    "Short ID: {} (auto-resolves on LAN)  ·  Address: {}",
                    dev.device_id.syncthing_prefix(),
                    dev.address
                )
            } else {
                format!(
                    "ID: {}  ·  Address: {}",
                    dev.device_id.to_syncthing_string(),
                    dev.address
                )
            };
            gfx.text_fit(
                lx + 78,
                cy + 12,
                w - 240,
                12,
                FontWeight::Regular,
                th.dim,
                Align::Left,
                &sub,
            );
        }
    }

    // Bottom modal action buttons
    let w_add = action_width(gfx, PadButton::Square, "+ Add by ID");
    let w_sync = action_width(gfx, PadButton::Triangle, "Apply & Sync");
    let w_close = action_width(gfx, PadButton::Circle, "Done");
    let bx_close = dx + w - 28 - w_close;
    let bx_sync = bx_close - 10 - w_sync;
    let bx_add = bx_sync - 10 - w_add;
    draw_action(
        gfx,
        th,
        bx_add,
        dy + h - 32,
        PadButton::Square,
        "+ Add by ID",
    );
    draw_action(
        gfx,
        th,
        bx_sync,
        dy + h - 32,
        PadButton::Triangle,
        "Apply & Sync",
    );
    draw_action(gfx, th, bx_close, dy + h - 32, PadButton::Circle, "Done");

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::DpadUd, None, "Select"),
            (PadButton::Cross, None, "Toggle"),
            (PadButton::Square, None, "+ Add by ID"),
            (PadButton::Triangle, None, "Apply & Sync"),
            (PadButton::Circle, None, "Done"),
        ],
    );
}
