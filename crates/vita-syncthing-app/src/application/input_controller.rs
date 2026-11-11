//! Input Controller translating Gamepad (`SceCtrl`) and Touchscreen (`SceTouch`) events on the
//! main screen into `AppState` transitions, `ShareService` use-case calls, and high-level
//! `UserCommand`s. Open modals are handled by `ModalInput`, D-pad focus by `DpadNavigation`.

use crate::application::dpad_navigation::{DpadNavigation, PageAction};
use crate::application::modal_input::{open_add_device_modal, ModalInput};
use crate::application::{FolderOfferService, ShareService};
use crate::domain::{ActiveTab, AppState, ModalDialog, PickerMode, BROWSER_ROWS, TAB_ORDER};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_L, BTN_R, BTN_SELECT, BTN_SQUARE, BTN_START,
    BTN_TRIANGLE,
};
use crate::ui::theme::{
    BROWSER_STEP, BROWSER_TOP, CONTENT_R, OPEN_CHIP_W, SETTINGS_CLEAR_BTN_X, SETTINGS_DECLINED_B,
    SETTINGS_DECLINED_Y,
};

/// High-level command returned by `InputController` to the main application loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserCommand {
    None,
    StartSync,
    OpenSystemKeyboard,
    QuitApp,
}

pub struct InputController;

impl InputController {
    /// Processes a single frame of user input (`SceCtrl` + `SceTouch`).
    pub fn handle_input(
        state: &mut AppState,
        repo: &ConfigRepository,
        input: InputFrame,
    ) -> UserCommand {
        let prev_tab = state.active_tab;
        let cmd = Self::handle_frame(state, repo, input);
        if state.active_tab != prev_tab {
            state.reset_nav_for_tab();
        }
        cmd
    }

    fn handle_frame(
        state: &mut AppState,
        repo: &ConfigRepository,
        input: InputFrame,
    ) -> UserCommand {
        // 0. If OLED screensaver is active, any button or touch contact wakes the display
        if state.oled_saver_active {
            if input.any_activity {
                state.oled_saver_active = false;
                ShareService::log_event(state, repo, "Exited OLED screensaver mode.");
            }
            return UserCommand::None;
        }

        // 1. If a Modal Dialog is open, route all gamepad & touch input to the modal
        if state.modal != ModalDialog::None {
            return ModalInput::handle(state, repo, input);
        }

        let mut trigger_sync = false;
        let mut toggle_share = false;
        let mut delete_share = false;
        let mut open_picker = false;

        // 2. Front touchscreen tap detection (960x544, VitaSend layout)
        if let Some((tx, ty)) = input.touch_tap {
            // Left Sidebar (x <= 212, y < 500)
            if tx <= 212 && ty < 500 {
                if (88..=134).contains(&ty) {
                    state.active_tab = ActiveTab::Dashboard;
                } else if (138..=184).contains(&ty) {
                    state.active_tab = ActiveTab::Shares;
                } else if (188..=234).contains(&ty) {
                    Self::switch_tab(state, repo, ActiveTab::FolderPicker);
                } else if (238..=284).contains(&ty) {
                    state.active_tab = ActiveTab::Settings;
                } else if (345..=440).contains(&ty) {
                    trigger_sync = true;
                }
            }
            // Bottom Hint Bar (y >= 500)
            else if ty >= 500 {
                if state.active_tab == ActiveTab::Shares {
                    if tx < 195 {
                        state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                    } else if tx < 420 {
                        if !state.shares.is_empty() {
                            state.modal = ModalDialog::ManageDevices {
                                share_idx: state.selected_share_idx,
                                selected_dev_idx: 0,
                            };
                        }
                    } else if tx < 610 {
                        delete_share = true;
                    } else if tx < 790 {
                        open_picker = true;
                    }
                } else if state.active_tab == ActiveTab::Settings {
                    if tx < 230 {
                        Self::toggle_theme(state, repo);
                    } else if tx < 420 {
                        state.modal = ModalDialog::ShowDeviceId;
                    } else if tx < 620 {
                        return open_add_device_modal(state, None);
                    } else if tx < 780 {
                        state.oled_saver_active = true;
                        return UserCommand::None;
                    } else {
                        state.modal = ModalDialog::ConfirmQuit;
                        return UserCommand::None;
                    }
                } else if tx < 160 {
                    trigger_sync = true;
                } else if tx < 330 {
                    state.modal = ModalDialog::ShowDeviceId;
                } else if tx < 500 {
                    toggle_share = true;
                } else if tx < 650 {
                    open_picker = true;
                } else if tx < 840 {
                    state.oled_saver_active = true;
                    return UserCommand::None;
                } else {
                    state.modal = ModalDialog::ConfirmQuit;
                    return UserCommand::None;
                }
            }
            // Main Content Area (x >= 236, y < 500)
            else if tx >= 236 {
                match state.active_tab {
                    ActiveTab::Dashboard => {
                        if (86..=238).contains(&ty) {
                            if ty >= 110 && tx < 785 {
                                state.modal = ModalDialog::ShowDeviceId;
                            } else {
                                trigger_sync = true;
                            }
                        }
                    }
                    ActiveTab::Shares => {
                        // Top-right "+ New Share" button (y in 24..68, x >= 760)
                        if (24..=68).contains(&ty) && tx >= 760 {
                            state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                        } else {
                            for i in 0..state.shares.len().min(4) {
                                let ry = 86 + i * 98;
                                if ty >= ry && ty <= ry + 90 {
                                    state.selected_share_idx = i;
                                    if ty >= ry + 42 && ty <= ry + 78 {
                                        if (580..=695).contains(&tx) {
                                            state.modal = ModalDialog::ManageDevices {
                                                share_idx: i,
                                                selected_dev_idx: 0,
                                            };
                                        } else if (696..=810).contains(&tx) {
                                            delete_share = true;
                                        } else if (811..=936).contains(&tx) {
                                            open_picker = true;
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    ActiveTab::FolderPicker => {
                        // Breadcrumb "Up" button at top right (y in 80..122, x >= 820)
                        if (80..=122).contains(&ty) && tx >= 820 {
                            ShareService::navigate_picker_up(state, repo);
                        } else {
                            for vis_i in 0..BROWSER_ROWS {
                                let idx = state.picker_top_idx + vis_i;
                                if idx >= state.picker_items.len() {
                                    break;
                                }
                                let ry = (BROWSER_TOP as usize) + vis_i * (BROWSER_STEP as usize);
                                if ty >= ry && ty <= ry + 38 {
                                    state.selected_picker_idx = idx;
                                    // The "Open" chip on the right enters the subfolder; the rest
                                    // of the row selects the folder.
                                    let on_open_chip = tx >= (CONTENT_R - OPEN_CHIP_W) as usize;
                                    ShareService::apply_picker_selection(state, repo, on_open_chip);
                                    break;
                                }
                            }
                        }
                    }
                    ActiveTab::Settings => {
                        if (86..=204).contains(&ty) {
                            if (470..=625).contains(&tx) {
                                return open_add_device_modal(state, None);
                            } else if (626..=760).contains(&tx) {
                                state.modal = ModalDialog::ShowDeviceId;
                            } else {
                                Self::toggle_theme(state, repo);
                            }
                        } else if (SETTINGS_DECLINED_Y as usize..=SETTINGS_DECLINED_B as usize)
                            .contains(&ty)
                            && tx >= SETTINGS_CLEAR_BTN_X as usize
                        {
                            FolderOfferService::clear_declined(state, repo);
                        }
                    }
                }
            }
        }

        // 3. Global PS Vita gamepad buttons
        if (input.pressed & BTN_START) != 0 {
            state.modal = ModalDialog::ConfirmQuit;
            return UserCommand::None;
        }
        if (input.pressed & BTN_SELECT) != 0 {
            state.oled_saver_active = !state.oled_saver_active;
            return UserCommand::None;
        }
        let tab_pos = TAB_ORDER
            .iter()
            .position(|&t| t == state.active_tab)
            .unwrap_or(0);
        if (input.pressed & BTN_L) != 0 {
            let tab = TAB_ORDER[(tab_pos + TAB_ORDER.len() - 1) % TAB_ORDER.len()];
            Self::switch_tab(state, repo, tab);
        }
        if (input.pressed & BTN_R) != 0 {
            Self::switch_tab(state, repo, TAB_ORDER[(tab_pos + 1) % TAB_ORDER.len()]);
        }

        // 4. D-pad focus navigation (sidebar <-> page) and `Cross` on the focused element
        let mut pressed = input.pressed;
        match DpadNavigation::handle(state, &mut pressed) {
            PageAction::None => {}
            PageAction::SwitchTab(tab) => Self::switch_tab(state, repo, tab),
            PageAction::StartSync => trigger_sync = true,
            PageAction::ShowDeviceId => state.modal = ModalDialog::ShowDeviceId,
            PageAction::ToggleTheme => Self::toggle_theme(state, repo),
            PageAction::AddDevice => return open_add_device_modal(state, None),
            PageAction::NewShare => state.modal = ModalDialog::CreateShare { selected_idx: 0 },
            PageAction::ManageDevices => {
                if state.selected_share_idx < state.shares.len() {
                    state.modal = ModalDialog::ManageDevices {
                        share_idx: state.selected_share_idx,
                        selected_dev_idx: 0,
                    };
                }
            }
            PageAction::DeleteShare => delete_share = true,
            PageAction::ChangeFolder => open_picker = true,
            PageAction::PickerSelect => ShareService::apply_picker_selection(state, repo, false),
            PageAction::PickerOpen => ShareService::apply_picker_selection(state, repo, true),
            PageAction::PickerUp => ShareService::navigate_picker_up(state, repo),
            PageAction::ClearDeclinedFolders => {
                FolderOfferService::clear_declined(state, repo);
            }
        }

        // 5. Tab-specific face-button shortcuts
        match state.active_tab {
            ActiveTab::Dashboard => {
                if (pressed & BTN_CIRCLE) != 0 {
                    state.modal = ModalDialog::ShowDeviceId;
                }
                if (pressed & BTN_SQUARE) != 0 {
                    toggle_share = true;
                }
                if (pressed & BTN_TRIANGLE) != 0 {
                    open_picker = true;
                }
            }
            ActiveTab::Shares => {
                if (pressed & BTN_CIRCLE) != 0 {
                    state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                }
                if (pressed & BTN_SQUARE) != 0 {
                    delete_share = true;
                }
                if (pressed & BTN_TRIANGLE) != 0 {
                    open_picker = true;
                }
            }
            ActiveTab::FolderPicker => {
                if (pressed & BTN_TRIANGLE) != 0 {
                    ShareService::apply_picker_selection(state, repo, true);
                }
                if (pressed & BTN_CIRCLE) != 0 {
                    ShareService::navigate_picker_up(state, repo);
                }
            }
            ActiveTab::Settings => {
                if (pressed & BTN_SQUARE) != 0 {
                    Self::toggle_theme(state, repo);
                }
                if (pressed & BTN_CIRCLE) != 0 {
                    state.modal = ModalDialog::ShowDeviceId;
                }
                if (pressed & BTN_TRIANGLE) != 0 {
                    return open_add_device_modal(state, None);
                }
            }
        }

        if toggle_share {
            ShareService::toggle_selected_share(state, repo);
        }

        if delete_share && state.selected_share_idx < state.shares.len() {
            state.modal = ModalDialog::ConfirmDeleteShare {
                share_idx: state.selected_share_idx,
            };
        }

        if open_picker {
            state.picker_mode = PickerMode::ChangeShareTarget;
            ShareService::refresh_picker_items(state, repo);
            state.active_tab = ActiveTab::FolderPicker;
        }

        if trigger_sync && !state.sync_in_progress {
            return UserCommand::StartSync;
        }

        UserCommand::None
    }

    /// Toggles the colour theme mode and persists the choice to disk.
    fn toggle_theme(state: &mut AppState, repo: &ConfigRepository) {
        state.toggle_theme();
        repo.save_theme(state.theme_mode);
    }

    /// Activates `tab`; the `Files` tab opens on the share-creation folder browser.
    fn switch_tab(state: &mut AppState, repo: &ConfigRepository, tab: ActiveTab) {
        state.active_tab = tab;
        if tab == ActiveTab::FolderPicker {
            state.picker_mode = PickerMode::CreateNewShare;
            ShareService::refresh_picker_items(state, repo);
        }
    }
}
