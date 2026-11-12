//! Top-level UI Renderer (`VitaUiRenderer`) owning `Gfx` and `VitaFramebuffer`.
//! Dispatches rendering of `&AppState` across tabs, modals, toast notifications, and OLED screensaver.

use crate::application::frame_pacing::ANIMATION_INTERVAL;
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
        let th = UiTheme::for_mode(state.theme_mode);

        if state.oled_saver_active {
            let steps = elapsed_vblanks.min(ANIMATION_INTERVAL);
            state.step_screensaver(GFX_W as f32, GFX_H as f32, SAVER_W, SAVER_H, steps);
            render_oled_screensaver(&mut self.gfx, &th, state);
            self.fb.present(&self.gfx.pixels);
            return;
        }

        draw_chrome(&mut self.gfx, &th, state);

        match state.active_tab {
            ActiveTab::Dashboard => render_dashboard_tab(&mut self.gfx, &th, state),
            ActiveTab::Shares => render_shares_tab(&mut self.gfx, &th, state),
            ActiveTab::FolderPicker => {
                state.clamp_picker_scroll();
                render_folder_picker_tab(&mut self.gfx, &th, state);
            }
            ActiveTab::Settings => render_settings_tab(&mut self.gfx, &th, state),
        }

        // Draw active modal dialog on top of dimmed screen
        match state.modal {
            ModalDialog::None => {}
            ModalDialog::CreateShare { selected_idx } => {
                render_create_share_modal(&mut self.gfx, &th, state, selected_idx);
            }
            ModalDialog::ManageDevices {
                share_idx,
                selected_dev_idx,
            } => {
                render_manage_devices_modal(&mut self.gfx, &th, state, share_idx, selected_dev_idx);
            }
            ModalDialog::AddDeviceById { keypad_idx, .. } => {
                render_add_device_modal(&mut self.gfx, &th, state, keypad_idx);
            }
            ModalDialog::ScanDeviceQr { .. } => {
                render_scan_qr_modal(&mut self.gfx, &th, state);
            }
            ModalDialog::ShowDeviceId => {
                render_device_id_modal(&mut self.gfx, &th, state);
            }
            ModalDialog::ConfirmQuit => {
                render_confirm_quit_modal(&mut self.gfx, &th, state);
            }
            ModalDialog::ConfirmDeleteShare { share_idx } => {
                render_confirm_delete_share_modal(&mut self.gfx, &th, state, share_idx);
            }
            ModalDialog::FolderOffer { offer_idx } => {
                render_folder_offer_modal(&mut self.gfx, &th, state, offer_idx);
            }
        }

        let s = strings::strings(state.language);
        draw_toast(&mut self.gfx, &th, s, state.toast_msg.as_ref());
        self.fb.present(&self.gfx.pixels);
    }

    /// Skips rendering for this VBlank: the previous frame stays on screen.
    pub fn idle(&mut self) {
        self.fb.wait_vblank();
    }
}
