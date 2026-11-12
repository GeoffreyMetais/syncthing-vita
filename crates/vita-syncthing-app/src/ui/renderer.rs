//! Top-level UI Renderer (`VitaUiRenderer`) owning `Gfx` and `VitaFramebuffer`.
//! Dispatches rendering of `&AppState` across tabs, modals, toast notifications, and OLED screensaver.

use crate::domain::defaults::ANIMATION_INTERVAL;
use crate::domain::{ActiveTab, AppState, ModalDialog};
use crate::infrastructure::VitaFramebuffer;
use crate::ui::gfx::{Gfx, GFX_H, GFX_W};
use crate::ui::modals::{
    render_add_device_modal, render_confirm_delete_share_modal, render_confirm_quit_modal,
    render_create_share_modal, render_device_id_modal, render_folder_offer_modal,
    render_manage_devices_modal, render_scan_qr_modal,
};
use crate::ui::screens::{
    render_dashboard_tab, render_folder_picker_tab, render_oled_screensaver, render_settings_tab,
    render_shares_tab, SAVER_H, SAVER_W,
};
use crate::ui::strings;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{draw_chrome, draw_toast};

/// Owns the 2D software canvas (`Gfx`) and hardware framebuffer (`VitaFramebuffer`).
pub struct VitaUiRenderer {
    fb: VitaFramebuffer,
    gfx: Gfx,
}

impl VitaUiRenderer {
    pub fn new() -> Self {
        Self {
            fb: VitaFramebuffer::new(),
            gfx: Gfx::new(),
        }
    }

    /// Renders a frame from `state` and presents it to the PS Vita display on the next VBlank.
    /// `elapsed_vblanks` is the number of display VBlanks since the previous frame; it keeps
    /// animations at the same speed whatever the rendering frame rate (see `FramePacer`).
    pub fn render(&mut self, state: &mut AppState, elapsed_vblanks: u32) {
        if state.oled_saver_active {
            let steps = elapsed_vblanks.min(ANIMATION_INTERVAL);
            state.step_screensaver(GFX_W as f32, GFX_H as f32, SAVER_W, SAVER_H, steps);
        }
        draw_frame(&mut self.gfx, state);
        self.fb.present(&self.gfx.pixels);
    }

    /// Skips rendering for this VBlank: the previous frame stays on screen.
    pub fn idle(&mut self) {
        self.fb.wait_vblank();
    }
}

/// Draws the frame for `state` into `gfx` (no display access, so it also runs in host tests)
/// and records its interactive elements in `state.hit_map`.
pub fn draw_frame(gfx: &mut Gfx, state: &mut AppState) {
    let th = UiTheme::for_mode(state.theme_mode);
    // The elements declared by this frame replace those of the previous one.
    let mut hits = std::mem::take(&mut state.hit_map);
    hits.clear();
    if state.active_tab == ActiveTab::FolderPicker {
        state.clamp_picker_scroll();
    }


    if state.oled_saver_active {
        render_oled_screensaver(gfx, &th, state);
        state.hit_map = hits;
        return;
    }

    draw_chrome(gfx, &th, state, &mut hits);

    match state.active_tab {
        ActiveTab::Dashboard => render_dashboard_tab(gfx, &th, state, &mut hits),
        ActiveTab::Shares => render_shares_tab(gfx, &th, state, &mut hits),
        ActiveTab::FolderPicker => {
            render_folder_picker_tab(gfx, &th, state, &mut hits);
        }
        ActiveTab::Settings => render_settings_tab(gfx, &th, state, &mut hits),
    }

    // Draw active modal dialog on top of dimmed screen. Nothing of the page behind it is
    // interactive while it is open.
    if state.modal != ModalDialog::None {
        hits.clear();
    }
    match state.modal {
        ModalDialog::None => {}
        ModalDialog::CreateShare { selected_idx } => {
            render_create_share_modal(gfx, &th, state, selected_idx, &mut hits);
        }
        ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx,
        } => {
            render_manage_devices_modal(gfx, &th, state, share_idx, selected_dev_idx, &mut hits);
        }
        ModalDialog::AddDeviceById { keypad_idx, .. } => {
            render_add_device_modal(gfx, &th, state, keypad_idx, &mut hits);
        }
        ModalDialog::ScanDeviceQr { .. } => {
            render_scan_qr_modal(gfx, &th, state, &mut hits);
        }
        ModalDialog::ShowDeviceId => {
            render_device_id_modal(gfx, &th, state, &mut hits);
        }
        ModalDialog::ConfirmQuit => {
            render_confirm_quit_modal(gfx, &th, state, &mut hits);
        }
        ModalDialog::ConfirmDeleteShare { share_idx } => {
            render_confirm_delete_share_modal(gfx, &th, state, share_idx, &mut hits);
        }
        ModalDialog::FolderOffer { offer_idx } => {
            render_folder_offer_modal(gfx, &th, state, offer_idx, &mut hits);
        }
    }

    let s = strings::strings(state.language);
    draw_toast(gfx, &th, s, state.toast_msg.as_ref());
    state.hit_map = hits;
}
