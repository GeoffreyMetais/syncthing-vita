//! Pure modal dialog views (`CreateShare`, `ManageDevices`, `ShowDeviceId`, `ConfirmQuit`).
//! Reads `&AppState` without mutating business state or touching filesystem I/O.

use qrcodegen::{QrCode, QrCodeEcc};

use crate::domain::{AppState, NEW_SHARE_TEMPLATES};
use crate::ui::gfx::{rgb, rgba, Align, FontWeight, Gfx, Icon, PadButton, GFX_W};
use crate::ui::theme::{UiTheme, C_BAD, C_GOOD, C_WARN, HINT_Y};
use crate::ui::widgets::{action_width, draw_action, draw_chip, draw_hints, draw_row_tick};

/// Modal Dialog 1: Create a new share on PS Vita (from template or custom `ux0:` folder).
pub fn render_create_share_modal(gfx: &mut Gfx, th: &UiTheme, selected_idx: usize) {
    gfx.dim(175);
    let w = 680;
    let h = 408;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Plus,
        (lx + 20) as f32,
        (dy + 40) as f32,
        22.0,
        th.accent_hi,
    );
    gfx.text(
        lx + 52,
        dy + 32,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        "Create New Share",
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "Choose a PS Vita folder preset or browse ux0: to create a new Syncthing share",
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    for (i, &(fid, label, path)) in NEW_SHARE_TEMPLATES.iter().enumerate() {
        let ry = dy + 90 + (i as i32) * 50;
        let cy = ry + 21;
        let is_sel = i == selected_idx;

        if is_sel {
            gfx.rrect(lx, ry, w - 56, 44, 10, th.accent_bg);
            gfx.rrect(lx, ry + 9, 4, 26, 2, th.accent);
        }

        let is_browse = fid == "__browse__";
        let ic = if is_browse {
            Icon::Receive
        } else {
            Icon::Folder
        };
        gfx.icon(
            ic,
            (lx + 24) as f32,
            cy as f32,
            22.0,
            if is_browse { C_GOOD } else { th.accent_hi },
        );

        gfx.text(
            lx + 48,
            cy - 7,
            16,
            if is_sel {
                FontWeight::Bold
            } else {
                FontWeight::Medium
            },
            th.text,
            Align::Left,
            label,
        );
        let sub = if is_browse {
            "Open the interactive ux0: directory browser".to_owned()
        } else {
            format!("ID: {}  ·  Path: {}", fid, path)
        };
        gfx.text(
            lx + 48,
            cy + 12,
            13,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            &sub,
        );
    }

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::DpadUd, None, "Select"),
            (PadButton::Cross, None, "Create & Pick Devices"),
            (PadButton::Circle, None, "Cancel"),
        ],
    );
}

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
            "Press Triangle (Scan & Sync) while Syncthing is open on your PC/Mac/Phone."
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
            let is_checked = shared_list.contains(&dev.device_id.to_hex());

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
            let sub = format!(
                "ID: {}  ·  Address: {}",
                dev.device_id.to_syncthing_string(),
                dev.address
            );
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
    let w_sync = action_width(gfx, PadButton::Triangle, "Apply & Sync");
    let w_close = action_width(gfx, PadButton::Circle, "Done");
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_sync - 12 - w_close,
        dy + h - 32,
        PadButton::Triangle,
        "Apply & Sync",
    );
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_close,
        dy + h - 32,
        PadButton::Circle,
        "Done",
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::DpadUd, None, "Select Device"),
            (PadButton::Cross, None, "Toggle Device"),
            (PadButton::Triangle, None, "Apply & Sync Now"),
            (PadButton::Circle, None, "Done"),
        ],
    );
}

/// Modal Dialog 3: Display the PS Vita's Syncthing Device ID + scannable QR code.
pub fn render_device_id_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    gfx.dim(180);
    let w = 720;
    let h = 414;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle((lx + 20) as f32, (dy + 40) as f32, 20.0, th.accent_bg);
    gfx.icon(
        Icon::Vita,
        (lx + 20) as f32,
        (dy + 40) as f32,
        26.0,
        th.accent_hi,
    );
    gfx.text(
        lx + 52,
        dy + 31,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        "PS Vita Syncthing Device ID",
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "Scan this QR code in Syncthing (Add Remote Device) or enter the 56-character ID",
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    // Left Column: Crisp white QR code card (`246 x 246`)
    let qr_box_x = lx;
    let qr_box_y = dy + 94;
    let qr_box_size = 246;
    gfx.rrect(
        qr_box_x,
        qr_box_y,
        qr_box_size,
        qr_box_size,
        14,
        rgb(255, 255, 255),
    );
    gfx.rrect_outline(qr_box_x, qr_box_y, qr_box_size, qr_box_size, 14, 1, th.border);

    let dev_id = &state.vita_device_id;
    if let Ok(qr) = QrCode::encode_text(dev_id, QrCodeEcc::Medium) {
        let n = qr.size();
        let scale = ((qr_box_size - 26) / n).max(3);
        let total_px = n * scale;
        let ox = qr_box_x + (qr_box_size - total_px) / 2;
        let oy = qr_box_y + (qr_box_size - total_px) / 2;
        let dark = rgb(10, 14, 18);

        for my in 0..n {
            for mx in 0..n {
                if qr.get_module(mx, my) {
                    gfx.rect(ox + mx * scale, oy + my * scale, scale, scale, dark);
                }
            }
        }
    }

    gfx.text(
        qr_box_x + qr_box_size / 2,
        qr_box_y + qr_box_size + 22,
        13,
        FontWeight::Medium,
        th.accent_hi,
        Align::Center,
        "Scan QR in Syncthing Web / Mobile UI",
    );

    // Right Column: Formatted 8-group Syncthing Device ID + Certificate details
    let rx = qr_box_x + qr_box_size + 24;
    let rw = dx + w - 28 - rx;

    gfx.text(
        rx,
        dy + 104,
        12,
        FontWeight::Bold,
        th.accent_hi,
        Align::Left,
        "SYNCTHING DEVICE ID (8 GROUPS OF 7 CHARS)",
    );

    let groups: Vec<&str> = dev_id.split('-').collect();
    let row1 = if groups.len() >= 4 {
        format!(
            "{}  -  {}  -  {}  -  {}",
            groups[0], groups[1], groups[2], groups[3]
        )
    } else {
        dev_id.clone()
    };
    let row2 = if groups.len() >= 8 {
        format!(
            "{}  -  {}  -  {}  -  {}",
            groups[4], groups[5], groups[6], groups[7]
        )
    } else {
        String::new()
    };

    // Row 1 box (Groups 1..4)
    let box1_y = dy + 120;
    gfx.rrect(rx, box1_y, rw, 50, 10, th.accent_bg);
    gfx.rrect_outline(rx, box1_y, rw, 50, 10, 1, th.accent);
    gfx.text(
        rx + 14,
        box1_y + 15,
        11,
        FontWeight::Medium,
        th.dim,
        Align::Left,
        "GROUPS 1 - 4",
    );
    gfx.text_fit(
        rx + 14,
        box1_y + 35,
        rw - 28,
        16,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &row1,
    );

    // Row 2 box (Groups 5..8)
    let box2_y = dy + 180;
    gfx.rrect(rx, box2_y, rw, 50, 10, th.accent_bg);
    gfx.rrect_outline(rx, box2_y, rw, 50, 10, 1, th.accent);
    gfx.text(
        rx + 14,
        box2_y + 15,
        11,
        FontWeight::Medium,
        th.dim,
        Align::Left,
        "GROUPS 5 - 8",
    );
    gfx.text_fit(
        rx + 14,
        box2_y + 35,
        rw - 28,
        16,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &row2,
    );

    // Device metadata card
    let meta_y = dy + 244;
    gfx.rrect(rx, meta_y, rw, 96, 10, th.bg);
    gfx.rrect_outline(rx, meta_y, rw, 96, 10, 1, th.border);

    let short_id = groups.first().copied().unwrap_or("VITA");
    draw_chip(gfx, rx + rw - 14, meta_y + 22, "TLS 1.3 · ECDSA P-256", C_GOOD);
    gfx.text(
        rx + 14,
        meta_y + 22,
        14,
        FontWeight::Bold,
        th.text,
        Align::Left,
        &format!("Short ID: {}", short_id),
    );
    gfx.text(
        rx + 14,
        meta_y + 48,
        13,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "Device Name: PlayStation Vita (vita-syncthing)",
    );
    gfx.text(
        rx + 14,
        meta_y + 72,
        13,
        FontWeight::Regular,
        th.faint,
        Align::Left,
        "Identity Cert: ux0:data/syncthing/cert.der",
    );

    // Close button at bottom-right of modal
    let w_close = action_width(gfx, PadButton::Circle, "Close");
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_close,
        dy + h - 32,
        PadButton::Circle,
        "Close",
    );

    draw_hints(
        gfx,
        th,
        &[(
            PadButton::Circle,
            Some(PadButton::Cross),
            "Close QR / Device ID",
        )],
    );
}

/// Modal Dialog 4: Confirmation before exiting VitaSync (`START` button).
pub fn render_confirm_quit_modal(gfx: &mut Gfx, th: &UiTheme, state: &AppState) {
    gfx.dim(180);
    let w = 520;
    let h = 236;
    let dx = (GFX_W - w) / 2;
    let dy = (HINT_Y - h) / 2;

    gfx.shadow(dx, dy, w, h, 18);
    gfx.rrect(dx, dy, w, h, 18, th.card);
    gfx.rrect_outline(dx, dy, w, h, 18, 1, th.border);

    let lx = dx + 28;
    gfx.circle(
        (lx + 20) as f32,
        (dy + 40) as f32,
        20.0,
        rgba(239, 83, 80, 42),
    );
    gfx.icon(Icon::Close, (lx + 20) as f32, (dy + 40) as f32, 22.0, C_BAD);
    gfx.text(
        lx + 52,
        dy + 31,
        21,
        FontWeight::Bold,
        th.text,
        Align::Left,
        "Quit VitaSync?",
    );
    gfx.text(
        lx + 52,
        dy + 54,
        14,
        FontWeight::Regular,
        th.dim,
        Align::Left,
        "Confirm whether to close the application and return to LiveArea",
    );
    gfx.line(
        (dx + 24) as f32,
        (dy + 76) as f32,
        (dx + w - 24) as f32,
        (dy + 76) as f32,
        1.0,
        th.border,
    );

    if state.sync_in_progress {
        gfx.text(
            lx,
            dy + 110,
            15,
            FontWeight::Medium,
            C_WARN,
            Align::Left,
            "A BEP/1.0 synchronization is currently in progress.",
        );
        gfx.text(
            lx,
            dy + 136,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            "Quitting now will interrupt active file transfers.",
        );
    } else {
        gfx.text(
            lx,
            dy + 112,
            15,
            FontWeight::Medium,
            th.text,
            Align::Left,
            "Are you sure you want to exit VitaSync?",
        );
        gfx.text(
            lx,
            dy + 138,
            14,
            FontWeight::Regular,
            th.dim,
            Align::Left,
            "All share configurations and device settings are saved on ux0:.",
        );
    }

    let w_quit = action_width(gfx, PadButton::Cross, "Quit App");
    let w_cancel = action_width(gfx, PadButton::Circle, "Cancel");
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_quit - 12 - w_cancel,
        dy + h - 34,
        PadButton::Circle,
        "Cancel",
    );
    draw_action(
        gfx,
        th,
        dx + w - 28 - w_quit,
        dy + h - 34,
        PadButton::Cross,
        "Quit App",
    );

    draw_hints(
        gfx,
        th,
        &[
            (PadButton::Cross, None, "Confirm Quit"),
            (PadButton::Circle, None, "Cancel"),
        ],
    );
}
