//! Input Controller: routes one frame of Gamepad (`SceCtrl`) and Touchscreen (`SceTouch`)
//! input on the main screen. What is interactive, where, and what it does is declared by the
//! screens in the frame's `HitMap`; this controller resolves touch, `Cross` on the focused
//! element and face-button shortcuts against it and executes the resulting `UiAction`.
//! Open modals are handled by `ModalInput`, D-pad focus by `DpadNavigation`.

use crate::application::dpad_navigation::DpadNavigation;
use crate::application::modal_input::{open_add_device_modal, ModalInput};
use crate::application::{
    DevicesScan, FilesController, LogsController, SettingsController, ShareService,
    SharesController,
};
use crate::domain::{
    ActiveTab, AppState, ModalDialog, PickerMode, ShortcutKey, UiAction, TAB_ORDER,
};
use crate::infrastructure::{
    ConfigRepository, InputFrame, BTN_CIRCLE, BTN_L, BTN_R, BTN_SQUARE, BTN_START, BTN_TRIANGLE,
};

/// Face buttons an element can declare as its shortcut.
const SHORTCUT_BUTTONS: [(u32, ShortcutKey); 3] = [
    (BTN_CIRCLE, ShortcutKey::Circle),
    (BTN_SQUARE, ShortcutKey::Square),
    (BTN_TRIANGLE, ShortcutKey::Triangle),
];

/// High-level command returned by `InputController` to the main application loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserCommand {
    None,
    StartSync,
    /// Look for Syncthing devices on the LAN now: asks the running worker for a discovery
    /// cycle, or starts a sync (whose first step is a discovery) when no worker runs.
    ScanLan,
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
        let prev_modal = state.modal;
        let mut cmd = Self::handle_frame(state, repo, input);
        if state.active_tab != prev_tab {
            state.reset_nav_for_tab();
        }
        let scan = DevicesScan::on_input(prev_modal, state);
        if cmd == UserCommand::None {
            cmd = scan;
        }
        cmd
    }

    fn handle_frame(
        state: &mut AppState,
        repo: &ConfigRepository,
        input: InputFrame,
    ) -> UserCommand {
        // 0. If OLED screensaver is active, any button press or touch contact wakes the display
        if state.oled_saver_active {
            if input.pressed != 0 || input.touch_pos.is_some() || input.touch_tap.is_some() {
                state.oled_saver_active = false;
                ShareService::log_event(state, repo, "Exited OLED screensaver mode.");
            }
            return UserCommand::None;
        }

        // 1. If a Modal Dialog is open, route all gamepad & touch input to the modal
        if state.modal != ModalDialog::None {
            return ModalInput::handle(state, repo, input);
        }

        // 2. Touch: the frame on screen declares what is under the finger (`HitMap`).
        if let Some((tx, ty)) = input.touch_tap {
            if let Some(action) = state.hit_map.action_at(tx as i32, ty as i32) {
                return Self::execute(state, repo, action);
            }
        }

        // 3. Global PS Vita gamepad buttons
        if (input.pressed & BTN_START) != 0 {
            return Self::execute(state, repo, UiAction::ConfirmQuit);
        }
        let mut pressed = input.pressed;
        if state.log_view.is_some() {
            // The log viewer covers the whole screen: D-pad and shoulder buttons scroll it.
            LogsController::scroll(state, &mut pressed);
        } else {
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
            if let Some(action) = DpadNavigation::handle(state, &mut pressed) {
                return Self::execute(state, repo, action);
            }
        }

        // 5. Face buttons trigger the element (button or hint) that displays them. On the
        // `Overview`, the buttons of a share row answer only while that row has the focus
        // (`Triangle` is `Folder` there, `New Share` elsewhere).
        let selected_row = match state.active_tab {
            _ if state.log_view.is_some() => None,
            ActiveTab::Dashboard => state
                .is_share_row_focused()
                .then_some(state.selected_share_idx),
            ActiveTab::FolderPicker => Some(state.selected_picker_idx),
            ActiveTab::Settings => Some(0),
        };
        let shortcut = SHORTCUT_BUTTONS
            .iter()
            .filter(|(button, _)| pressed & button != 0)
            .find_map(|(_, key)| state.hit_map.shortcut_action(*key, selected_row));
        match shortcut {
            Some(action) => Self::execute(state, repo, action),
            None => UserCommand::None,
        }
    }

    /// Executes `action`: global actions here, page actions by the page's controller.
    pub fn execute(state: &mut AppState, repo: &ConfigRepository, action: UiAction) -> UserCommand {
        match action {
            UiAction::SwitchTab(tab) => Self::switch_tab(state, repo, tab),
            UiAction::StartSync => {
                if !state.sync_in_progress {
                    return UserCommand::StartSync;
                }
            }
            UiAction::ShowDeviceId => state.modal = ModalDialog::ShowDeviceId,
            UiAction::AddDevice => return open_add_device_modal(state, None),
            UiAction::ConfirmQuit => state.modal = ModalDialog::ConfirmQuit,
            UiAction::NewShare
            | UiAction::SelectShare(_)
            | UiAction::ManageDevices(_)
            | UiAction::DeleteShare(_)
            | UiAction::ChangeShareFolder(_) => SharesController::apply(state, repo, action),
            UiAction::PickerUp | UiAction::PickerSelect(_) | UiAction::PickerOpen(_) => {
                FilesController::apply(state, repo, action)
            }
            UiAction::ToggleTheme | UiAction::CycleLanguage | UiAction::ClearDeclinedFolders => {
                SettingsController::apply(state, repo, action)
            }
            UiAction::OpenLogs
            | UiAction::CloseLogs
            | UiAction::ToggleLogHistory
            | UiAction::ScrollLogsOlder
            | UiAction::ScrollLogsNewer => LogsController::apply(state, repo, action),
            // Elements of modal dialogs, resolved by `ModalInput`.
            UiAction::Press(_)
            | UiAction::ModalRow(_)
            | UiAction::OpenKeyboard
            | UiAction::Inert => {}
        }
        UserCommand::None
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
