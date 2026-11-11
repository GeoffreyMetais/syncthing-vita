//! Input Controller translating Gamepad (`SceCtrl`) and Touchscreen (`SceTouch`) events
//! into `AppState` transitions, `ShareService` use-case calls, and high-level `UserCommand`s.

use crate::application::{ShareCreation, ShareService};
use crate::domain::{
    ActiveTab, AppState, ModalDialog, PickerMode, ADD_DEVICE_KEYPAD, BROWSER_ROWS,
    NEW_SHARE_TEMPLATES,
};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_LEFT, BTN_R,
    BTN_RIGHT, BTN_SELECT, BTN_SQUARE, BTN_START, BTN_TRIANGLE, BTN_UP,
};
use crate::ui::theme::{BROWSER_STEP, BROWSER_TOP, CONTENT_R, OPEN_CHIP_W};

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
            return Self::handle_modal_input(state, repo, input);
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
                    state.picker_mode = PickerMode::CreateNewShare;
                    ShareService::refresh_picker_items(state, repo);
                    state.active_tab = ActiveTab::FolderPicker;
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
                        state.toggle_theme();
                    } else if tx < 420 {
                        state.modal = ModalDialog::ShowDeviceId;
                    } else if tx < 620 {
                        return Self::open_add_device_modal(state, None);
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
                                return Self::open_add_device_modal(state, None);
                            } else if (626..=760).contains(&tx) {
                                state.modal = ModalDialog::ShowDeviceId;
                            } else {
                                state.toggle_theme();
                            }
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
        if (input.pressed & BTN_L) != 0 {
            state.active_tab = match state.active_tab {
                ActiveTab::Dashboard => ActiveTab::Settings,
                ActiveTab::Shares => ActiveTab::Dashboard,
                ActiveTab::FolderPicker => ActiveTab::Shares,
                ActiveTab::Settings => ActiveTab::FolderPicker,
            };
            if state.active_tab == ActiveTab::FolderPicker {
                state.picker_mode = PickerMode::CreateNewShare;
                ShareService::refresh_picker_items(state, repo);
            }
        }
        if (input.pressed & BTN_R) != 0 {
            state.active_tab = match state.active_tab {
                ActiveTab::Dashboard => ActiveTab::Shares,
                ActiveTab::Shares => ActiveTab::FolderPicker,
                ActiveTab::FolderPicker => ActiveTab::Settings,
                ActiveTab::Settings => ActiveTab::Dashboard,
            };
            if state.active_tab == ActiveTab::FolderPicker {
                state.picker_mode = PickerMode::CreateNewShare;
                ShareService::refresh_picker_items(state, repo);
            }
        }

        // 4. Tab-specific actions
        match state.active_tab {
            ActiveTab::Dashboard => {
                if (input.pressed & BTN_CROSS) != 0 {
                    trigger_sync = true;
                }
                if (input.pressed & BTN_CIRCLE) != 0 {
                    state.modal = ModalDialog::ShowDeviceId;
                }
                if (input.pressed & BTN_SQUARE) != 0 {
                    toggle_share = true;
                }
                if (input.pressed & BTN_TRIANGLE) != 0 {
                    open_picker = true;
                }
            }
            ActiveTab::Shares => {
                if (input.pressed & BTN_UP) != 0 && state.selected_share_idx > 0 {
                    state.selected_share_idx -= 1;
                }
                if (input.pressed & BTN_DOWN) != 0
                    && state.selected_share_idx + 1 < state.shares.len()
                {
                    state.selected_share_idx += 1;
                }
                if (input.pressed & BTN_CIRCLE) != 0 {
                    state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                }
                if (input.pressed & BTN_CROSS) != 0 {
                    if state.shares.is_empty() {
                        state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                    } else {
                        state.modal = ModalDialog::ManageDevices {
                            share_idx: state.selected_share_idx,
                            selected_dev_idx: 0,
                        };
                    }
                }
                if (input.pressed & BTN_SQUARE) != 0 {
                    delete_share = true;
                }
                if (input.pressed & BTN_TRIANGLE) != 0 {
                    open_picker = true;
                }
            }
            ActiveTab::FolderPicker => {
                if (input.pressed & BTN_UP) != 0 && state.selected_picker_idx > 0 {
                    state.selected_picker_idx -= 1;
                    state.clamp_picker_scroll();
                }
                if (input.pressed & BTN_DOWN) != 0
                    && state.selected_picker_idx + 1 < state.picker_items.len()
                {
                    state.selected_picker_idx += 1;
                    state.clamp_picker_scroll();
                }
                if (input.pressed & BTN_CROSS) != 0 {
                    ShareService::apply_picker_selection(state, repo, false);
                }
                if (input.pressed & BTN_TRIANGLE) != 0 {
                    ShareService::apply_picker_selection(state, repo, true);
                }
                if (input.pressed & BTN_CIRCLE) != 0 {
                    ShareService::navigate_picker_up(state, repo);
                }
            }
            ActiveTab::Settings => {
                if (input.pressed & BTN_CROSS) != 0 || (input.pressed & BTN_SQUARE) != 0 {
                    state.toggle_theme();
                }
                if (input.pressed & BTN_CIRCLE) != 0 {
                    state.modal = ModalDialog::ShowDeviceId;
                }
                if (input.pressed & BTN_TRIANGLE) != 0 {
                    return Self::open_add_device_modal(state, None);
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

    fn handle_modal_input(
        state: &mut AppState,
        repo: &ConfigRepository,
        input: InputFrame,
    ) -> UserCommand {
        match state.modal {
            ModalDialog::None => UserCommand::None,
            ModalDialog::CreateShare { mut selected_idx } => {
                let count = NEW_SHARE_TEMPLATES.len();
                if (input.pressed & BTN_UP) != 0 && selected_idx > 0 {
                    selected_idx -= 1;
                    state.modal = ModalDialog::CreateShare { selected_idx };
                }
                if (input.pressed & BTN_DOWN) != 0 && selected_idx + 1 < count {
                    selected_idx += 1;
                    state.modal = ModalDialog::CreateShare { selected_idx };
                }
                if (input.pressed & BTN_CIRCLE) != 0 {
                    state.modal = ModalDialog::None;
                    return UserCommand::None;
                }

                let mut confirm = (input.pressed & BTN_CROSS) != 0;
                if let Some((tx, ty)) = input.touch_tap {
                    let w = 680usize;
                    let h = 408usize;
                    let dx = (960 - w) / 2;
                    let dy = (500 - h) / 2;
                    if tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h {
                        for i in 0..count {
                            let ry = dy + 90 + i * 50;
                            if ty >= ry && ty <= ry + 44 {
                                selected_idx = i;
                                state.modal = ModalDialog::CreateShare { selected_idx };
                                confirm = true;
                                break;
                            }
                        }
                    } else {
                        state.modal = ModalDialog::None;
                        return UserCommand::None;
                    }
                }

                if confirm {
                    let (fid, label, path) = NEW_SHARE_TEMPLATES[selected_idx];
                    if fid == "__browse__" {
                        state.modal = ModalDialog::None;
                        state.picker_mode = PickerMode::CreateNewShare;
                        ShareService::refresh_picker_items(state, repo);
                        state.active_tab = ActiveTab::FolderPicker;
                    } else {
                        ShareCreation::begin(state, repo, fid, label, path);
                    }
                }
                UserCommand::None
            }
            ModalDialog::ManageDevices {
                share_idx,
                mut selected_dev_idx,
            } => {
                let dev_count = state.devices.len().min(5);
                if (input.pressed & BTN_UP) != 0 && selected_dev_idx > 0 {
                    selected_dev_idx -= 1;
                    state.modal = ModalDialog::ManageDevices {
                        share_idx,
                        selected_dev_idx,
                    };
                }
                if (input.pressed & BTN_DOWN) != 0 && selected_dev_idx + 1 < dev_count {
                    selected_dev_idx += 1;
                    state.modal = ModalDialog::ManageDevices {
                        share_idx,
                        selected_dev_idx,
                    };
                }
                if (input.pressed & BTN_CIRCLE) != 0 {
                    Self::close_manage_devices(state, repo);
                    return UserCommand::None;
                }
                if (input.pressed & BTN_SQUARE) != 0 {
                    return Self::open_add_device_modal(state, Some(share_idx));
                }
                if (input.pressed & BTN_SELECT) != 0 {
                    state.selected_share_idx = share_idx;
                    ShareService::toggle_selected_share(state, repo);
                    return UserCommand::None;
                }
                if (input.pressed & BTN_TRIANGLE) != 0 {
                    return Self::apply_and_sync(state, repo, share_idx);
                }

                let mut toggle_dev = (input.pressed & BTN_CROSS) != 0;
                if let Some((tx, ty)) = input.touch_tap {
                    let w = 680usize;
                    let h = 390usize;
                    let dx = (960 - w) / 2;
                    let dy = (500 - h) / 2;
                    if ty >= 500 && (320..=520).contains(&tx) {
                        return Self::open_add_device_modal(state, Some(share_idx));
                    }
                    if tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h {
                        // Bottom buttons inside modal (+ Add by ID / Apply & Sync / Done)
                        if ty >= dy + h - 56 {
                            if tx >= dx + w - 140 {
                                Self::close_manage_devices(state, repo);
                            } else if tx >= dx + w - 330 {
                                return Self::apply_and_sync(state, repo, share_idx);
                            } else if tx >= dx + w - 500 {
                                return Self::open_add_device_modal(state, Some(share_idx));
                            } else {
                                state.selected_share_idx = share_idx;
                                ShareService::toggle_selected_share(state, repo);
                            }
                            return UserCommand::None;
                        }
                        for i in 0..dev_count {
                            let ry = dy + 92 + i * 52;
                            if ty >= ry && ty <= ry + 46 {
                                selected_dev_idx = i;
                                state.modal = ModalDialog::ManageDevices {
                                    share_idx,
                                    selected_dev_idx,
                                };
                                toggle_dev = true;
                                break;
                            }
                        }
                    } else {
                        Self::close_manage_devices(state, repo);
                        return UserCommand::None;
                    }
                }

                if toggle_dev && selected_dev_idx < state.devices.len() {
                    let dev_id = state.devices[selected_dev_idx].device_id;
                    if let Some((folder_lbl, dev_name, added)) =
                        ShareService::toggle_device_for_share(state, repo, share_idx, dev_id)
                    {
                        let action_str = if added { "Added" } else { "Removed" };
                        let prep = if added { "to" } else { "from" };
                        let msg = format!("{} {} {} '{}'", action_str, dev_name, prep, folder_lbl);
                        state.show_toast(&msg);
                        ShareService::log_event(state, repo, &msg);
                    }
                }
                UserCommand::None
            }
            ModalDialog::AddDeviceById {
                return_share_idx,
                mut keypad_idx,
            } => {
                // 1. Live events from the native PS Vita System Keyboard (`SceIme`)
                if let Some(ime_str) = input.ime_text {
                    state.device_id_input = ime_str.trim().to_uppercase();
                }
                if input.ime_submitted {
                    state.ime_active = false;
                    return Self::confirm_add_device(state, repo, return_share_idx);
                }
                if input.ime_canceled {
                    state.ime_active = false;
                    return UserCommand::None;
                }

                // 2. Gamepad actions when the OS keyboard overlay is closed
                if (input.pressed & BTN_TRIANGLE) != 0 {
                    return UserCommand::OpenSystemKeyboard;
                }
                if (input.pressed & BTN_START) != 0 {
                    return Self::confirm_add_device(state, repo, return_share_idx);
                }
                if (input.pressed & BTN_CIRCLE) != 0 {
                    Self::close_add_device_modal(state, return_share_idx);
                    return UserCommand::None;
                }
                if (input.pressed & BTN_SQUARE) != 0 {
                    state.device_id_input.pop();
                }

                // 9x4 Keypad navigation
                if (input.pressed & BTN_LEFT) != 0 && keypad_idx % 9 > 0 {
                    keypad_idx -= 1;
                    state.modal = ModalDialog::AddDeviceById {
                        return_share_idx,
                        keypad_idx,
                    };
                }
                if (input.pressed & BTN_RIGHT) != 0 && keypad_idx % 9 < 8 {
                    keypad_idx += 1;
                    state.modal = ModalDialog::AddDeviceById {
                        return_share_idx,
                        keypad_idx,
                    };
                }
                if (input.pressed & BTN_UP) != 0 && keypad_idx >= 9 {
                    keypad_idx -= 9;
                    state.modal = ModalDialog::AddDeviceById {
                        return_share_idx,
                        keypad_idx,
                    };
                }
                if (input.pressed & BTN_DOWN) != 0 && keypad_idx + 9 < ADD_DEVICE_KEYPAD.len() {
                    keypad_idx += 9;
                    state.modal = ModalDialog::AddDeviceById {
                        return_share_idx,
                        keypad_idx,
                    };
                }

                let mut activate_key = (input.pressed & BTN_CROSS) != 0;
                if let Some((tx, ty)) = input.touch_tap {
                    let w = 700usize;
                    let h = 424usize;
                    let dx = (960 - w) / 2;
                    let dy = (500 - h) / 2;
                    let lx = dx + 28;

                    if ty >= 500 {
                        if tx < 220 {
                            return UserCommand::OpenSystemKeyboard;
                        } else if tx < 420 {
                            return Self::confirm_add_device(state, repo, return_share_idx);
                        } else if tx >= 760 {
                            Self::close_add_device_modal(state, return_share_idx);
                        }
                        return UserCommand::None;
                    }

                    if tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h {
                        // Tapping the text input box opens the PS Vita System Keyboard
                        if (dy + 76..=dy + 132).contains(&ty) {
                            return UserCommand::OpenSystemKeyboard;
                        }
                        // Tapping bottom action buttons (`Add Device`, `Backspace`, `Cancel`)
                        if ty >= dy + h - 52 {
                            if tx >= dx + w - 135 {
                                Self::close_add_device_modal(state, return_share_idx);
                            } else if tx >= dx + w - 275 {
                                state.device_id_input.pop();
                            } else if tx >= dx + w - 440 {
                                return Self::confirm_add_device(state, repo, return_share_idx);
                            }
                            return UserCommand::None;
                        }
                        // Tapping a key in the 9x4 Base32 keypad grid
                        let grid_y = dy + 162;
                        for idx in 0..ADD_DEVICE_KEYPAD.len() {
                            let col = idx % 9;
                            let row = idx / 9;
                            let kx = lx + col * 72;
                            let ky = grid_y + row * 42;
                            if tx >= kx && tx <= kx + 68 && ty >= ky && ty <= ky + 38 {
                                keypad_idx = idx;
                                state.modal = ModalDialog::AddDeviceById {
                                    return_share_idx,
                                    keypad_idx,
                                };
                                activate_key = true;
                                break;
                            }
                        }
                    } else {
                        Self::close_add_device_modal(state, return_share_idx);
                        return UserCommand::None;
                    }
                }

                if activate_key {
                    let key_label = ADD_DEVICE_KEYPAD[keypad_idx];
                    match key_label {
                        "Bksp" => {
                            state.device_id_input.pop();
                        }
                        "Clear" => {
                            state.device_id_input.clear();
                        }
                        "Add" => {
                            return Self::confirm_add_device(state, repo, return_share_idx);
                        }
                        ch => {
                            if state.device_id_input.len() < 63 {
                                state.device_id_input.push_str(ch);
                            }
                        }
                    }
                }

                UserCommand::None
            }
            ModalDialog::ShowDeviceId => {
                if (input.pressed & BTN_SQUARE) != 0 {
                    return Self::open_add_device_modal(state, None);
                }
                if let Some((tx, ty)) = input.touch_tap {
                    let w = 720usize;
                    let h = 414usize;
                    let dx = (960 - w) / 2;
                    let dy = (500 - h) / 2;
                    if (ty >= dy + h - 56
                        && ty <= dy + h - 10
                        && tx >= dx + w - 350
                        && tx <= dx + w - 130)
                        || (ty >= 500 && tx < 320)
                    {
                        return Self::open_add_device_modal(state, None);
                    }
                    state.modal = ModalDialog::None;
                    return UserCommand::None;
                }
                if (input.pressed & (BTN_CIRCLE | BTN_CROSS | BTN_TRIANGLE)) != 0 {
                    state.modal = ModalDialog::None;
                }
                UserCommand::None
            }
            ModalDialog::ConfirmDeleteShare { share_idx } => {
                let mut confirm = (input.pressed & BTN_CROSS) != 0;
                let mut cancel = (input.pressed & (BTN_CIRCLE | BTN_SQUARE | BTN_TRIANGLE)) != 0;
                if let Some((tx, ty)) = input.touch_tap {
                    let w = 520usize;
                    let h = 236usize;
                    let dx = (960 - w) / 2;
                    let dy = (500 - h) / 2;
                    let on_delete_btn = tx >= dx + w - 160
                        && tx <= dx + w - 16
                        && ty >= dy + h - 56
                        && ty <= dy + h - 12;
                    if on_delete_btn || (ty >= 500 && tx < 200) {
                        confirm = true;
                    } else {
                        cancel = true;
                    }
                }
                if confirm {
                    state.modal = ModalDialog::None;
                    if let Some(label) = ShareService::delete_share(state, repo, share_idx) {
                        state.show_toast(&format!("Deleted share '{}'", label));
                    }
                } else if cancel {
                    state.modal = ModalDialog::None;
                }
                UserCommand::None
            }
            ModalDialog::ConfirmQuit => {
                if (input.pressed & BTN_CROSS) != 0 {
                    return UserCommand::QuitApp;
                }
                if (input.pressed & (BTN_CIRCLE | BTN_START | BTN_SQUARE | BTN_TRIANGLE)) != 0 {
                    state.modal = ModalDialog::None;
                    return UserCommand::None;
                }
                if let Some((tx, ty)) = input.touch_tap {
                    let w = 520usize;
                    let h = 236usize;
                    let dx = (960 - w) / 2;
                    let dy = (500 - h) / 2;
                    // Check if user tapped "Quit App" button at bottom-right of modal
                    if tx >= dx + w - 160
                        && tx <= dx + w - 16
                        && ty >= dy + h - 56
                        && ty <= dy + h - 12
                    {
                        return UserCommand::QuitApp;
                    }
                    // Check if user tapped "Confirm Quit" in bottom hint bar
                    if ty >= 500 && tx < 200 {
                        return UserCommand::QuitApp;
                    }
                    state.modal = ModalDialog::None;
                }
                UserCommand::None
            }
        }
    }

    /// Closes `ManageDevices`, discarding a share being created that still has no linked device.
    fn close_manage_devices(state: &mut AppState, repo: &ConfigRepository) {
        state.modal = ModalDialog::None;
        ShareCreation::finish(state, repo);
    }

    /// `Apply & Sync` in `ManageDevices`: offers the share to its devices and (re)starts the sync.
    /// With no known device yet, keeps the modal open while the LAN is scanned.
    fn apply_and_sync(
        state: &mut AppState,
        repo: &ConfigRepository,
        share_idx: usize,
    ) -> UserCommand {
        if state.is_share_creation_incomplete(share_idx) && !state.devices.is_empty() {
            state.show_toast("Link at least one device to create this share");
            return UserCommand::None;
        }
        if let Some(share) = state.shares.get(share_idx) {
            if share.enabled && !share.shared_devices.is_empty() {
                let fid = share.folder_id.clone();
                state.queue_folder_offer(&fid);
                state.config_dirty = true;
            }
        }
        if !state.devices.is_empty() {
            Self::close_manage_devices(state, repo);
        }
        if !state.sync_in_progress {
            return UserCommand::StartSync;
        }
        UserCommand::None
    }

    fn open_add_device_modal(state: &mut AppState, return_share_idx: Option<usize>) -> UserCommand {
        state.device_id_input.clear();
        state.modal = ModalDialog::AddDeviceById {
            return_share_idx,
            keypad_idx: 0,
        };
        UserCommand::OpenSystemKeyboard
    }

    fn close_add_device_modal(state: &mut AppState, return_share_idx: Option<usize>) {
        state.ime_active = false;
        if let Some(share_idx) = return_share_idx {
            state.modal = ModalDialog::ManageDevices {
                share_idx,
                selected_dev_idx: 0,
            };
        } else {
            state.modal = ModalDialog::None;
        }
    }

    fn confirm_add_device(
        state: &mut AppState,
        repo: &ConfigRepository,
        return_share_idx: Option<usize>,
    ) -> UserCommand {
        let input_str = state.device_id_input.clone();
        match ShareService::add_device_by_input(state, repo, &input_str, return_share_idx) {
            Ok((dev_id, dev_name, is_short)) => {
                let msg = if is_short {
                    format!(
                        "Added Short ID {} (will auto-pair on LAN)",
                        dev_id.syncthing_prefix()
                    )
                } else {
                    format!("Added device {} ({})", dev_name, dev_id.short())
                };
                ShareService::log_event(state, repo, &msg);
                state.show_toast(&msg);

                if let Some(share_idx) = return_share_idx {
                    let selected_dev_idx = state
                        .devices
                        .iter()
                        .position(|d| d.device_id == dev_id)
                        .unwrap_or(0)
                        .min(4);
                    state.modal = ModalDialog::ManageDevices {
                        share_idx,
                        selected_dev_idx,
                    };
                } else {
                    state.modal = ModalDialog::None;
                }

                if !state.sync_in_progress {
                    UserCommand::StartSync
                } else {
                    UserCommand::None
                }
            }
            Err(err) => {
                state.show_toast(&err);
                UserCommand::None
            }
        }
    }
}
