//! D-pad navigation on the main screen (`DpadNavigation`): moves the focus between the sidebar
//! tabs and the current page's elements, and turns `Cross` into the focused element's action.

use crate::domain::{
    action_count, default_col, ActiveTab, AppState, NavFocus, SETTINGS_ROWS, TAB_ORDER,
};
use crate::infrastructure::{BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_UP};

/// Action requested by D-pad navigation, executed by `InputController`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageAction {
    None,
    SwitchTab(ActiveTab),
    StartSync,
    ShowDeviceId,
    ToggleTheme,
    AddDevice,
    NewShare,
    ManageDevices,
    DeleteShare,
    ChangeFolder,
    PickerSelect,
    PickerOpen,
    PickerUp,
    ClearDeclinedFolders,
}

/// Buttons handled here; they are removed from the frame so page shortcuts do not see them.
const NAV_BUTTONS: u32 = BTN_UP | BTN_DOWN | BTN_LEFT | BTN_RIGHT | BTN_CROSS;

pub struct DpadNavigation;

impl DpadNavigation {
    /// Handles D-pad and `Cross` on the main screen (no modal open) and clears them from `pressed`.
    pub fn handle(state: &mut AppState, pressed: &mut u32) -> PageAction {
        let buttons = *pressed & NAV_BUTTONS;
        *pressed &= !NAV_BUTTONS;
        if buttons == 0 {
            return PageAction::None;
        }
        match state.nav.focus {
            NavFocus::Sidebar => Self::sidebar(state, buttons),
            NavFocus::Content => match state.active_tab {
                ActiveTab::Shares => Self::shares(state, buttons),
                ActiveTab::FolderPicker => Self::files(state, buttons),
                ActiveTab::Settings => Self::settings(state, buttons),
                tab => Self::action_row(state, tab, buttons),
            },
        }
    }

    fn sidebar(state: &mut AppState, buttons: u32) -> PageAction {
        if buttons & (BTN_RIGHT | BTN_CROSS) != 0 {
            state.nav.focus = NavFocus::Content;
            return PageAction::None;
        }
        let pos = TAB_ORDER
            .iter()
            .position(|&t| t == state.active_tab)
            .unwrap_or(0);
        if buttons & BTN_UP != 0 && pos > 0 {
            return PageAction::SwitchTab(TAB_ORDER[pos - 1]);
        }
        if buttons & BTN_DOWN != 0 && pos + 1 < TAB_ORDER.len() {
            return PageAction::SwitchTab(TAB_ORDER[pos + 1]);
        }
        PageAction::None
    }

    /// `Left` on the first button goes back to the sidebar; `Right` moves along the row.
    fn move_in_row(state: &mut AppState, tab: ActiveTab, buttons: u32) {
        if buttons & BTN_LEFT != 0 {
            if state.nav.col == 0 || state.nav.header {
                state.nav.focus = NavFocus::Sidebar;
            } else {
                state.nav.col -= 1;
            }
        }
        if buttons & BTN_RIGHT != 0 && !state.nav.header && state.nav.col + 1 < action_count(tab) {
            state.nav.col += 1;
        }
    }

    /// Dashboard (`[QR / ID, Sync Now]`) and Settings (`[+ Add Device, QR / ID, Switch Theme]`).
    fn action_row(state: &mut AppState, tab: ActiveTab, buttons: u32) -> PageAction {
        Self::move_in_row(state, tab, buttons);
        if buttons & BTN_CROSS == 0 {
            return PageAction::None;
        }
        match (tab, state.nav.col) {
            (ActiveTab::Dashboard, 0) => PageAction::ShowDeviceId,
            (ActiveTab::Dashboard, _) => PageAction::StartSync,
            (_, 0) => PageAction::AddDevice,
            (_, 1) => PageAction::ShowDeviceId,
            _ => PageAction::ToggleTheme,
        }
    }

    /// Settings: the display card's `[+ Add Device, QR / ID, Switch Theme]` row, then the
    /// declined folders card's `Clear Declined` button.
    fn settings(state: &mut AppState, buttons: u32) -> PageAction {
        if buttons & BTN_UP != 0 && state.nav.row > 0 {
            state.nav.row -= 1;
            state.nav.col = default_col(ActiveTab::Settings);
        }
        if buttons & BTN_DOWN != 0 && state.nav.row + 1 < SETTINGS_ROWS {
            state.nav.row += 1;
            state.nav.col = 0;
        }
        if state.nav.row == 0 {
            return Self::action_row(state, ActiveTab::Settings, buttons);
        }
        if buttons & BTN_LEFT != 0 {
            state.nav.focus = NavFocus::Sidebar;
        }
        if buttons & BTN_CROSS != 0 {
            PageAction::ClearDeclinedFolders
        } else {
            PageAction::None
        }
    }

    /// Shares: `+ New Share` header, then one row per share with `[Devices, Delete, Folder]`.
    fn shares(state: &mut AppState, buttons: u32) -> PageAction {
        let count = state.shares.len();
        if count == 0 {
            state.nav.header = true;
        }
        if buttons & BTN_UP != 0 && !state.nav.header {
            if state.selected_share_idx == 0 {
                state.nav.header = true;
            } else {
                state.selected_share_idx -= 1;
            }
        }
        if buttons & BTN_DOWN != 0 {
            if state.nav.header {
                state.nav.header = count == 0;
            } else if state.selected_share_idx + 1 < count {
                state.selected_share_idx += 1;
            }
        }
        Self::move_in_row(state, ActiveTab::Shares, buttons);
        if buttons & BTN_CROSS == 0 {
            return PageAction::None;
        }
        if state.nav.header {
            return PageAction::NewShare;
        }
        match state.nav.col {
            0 => PageAction::ManageDevices,
            1 => PageAction::DeleteShare,
            _ => PageAction::ChangeFolder,
        }
    }

    /// Files: `Up` header button, then the folder rows (`Right` opens a folder).
    fn files(state: &mut AppState, buttons: u32) -> PageAction {
        if buttons & BTN_UP != 0 && !state.nav.header {
            if state.selected_picker_idx == 0 {
                state.nav.header = true;
            } else {
                state.selected_picker_idx -= 1;
                state.clamp_picker_scroll();
            }
        }
        if buttons & BTN_DOWN != 0 {
            if state.nav.header {
                state.nav.header = false;
            } else if state.selected_picker_idx + 1 < state.picker_items.len() {
                state.selected_picker_idx += 1;
                state.clamp_picker_scroll();
            }
        }
        if buttons & BTN_LEFT != 0 {
            state.nav.focus = NavFocus::Sidebar;
            return PageAction::None;
        }
        if buttons & BTN_RIGHT != 0 && !state.nav.header {
            return PageAction::PickerOpen;
        }
        if buttons & BTN_CROSS == 0 {
            return PageAction::None;
        }
        if state.nav.header {
            PageAction::PickerUp
        } else {
            PageAction::PickerSelect
        }
    }
}
