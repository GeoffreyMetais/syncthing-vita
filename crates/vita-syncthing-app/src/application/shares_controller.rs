//! `Shares` page controller: D-pad focus over the page's elements and execution of their
//! `UiAction`s. The elements themselves (position, action, focus slot, shortcut) are declared
//! by `ui/screens/shares.rs` in the frame's `HitMap`.

use crate::application::ShareService;
use crate::domain::{
    action_count, ActiveTab, AppState, FocusSlot, ModalDialog, NavFocus, PickerMode, ShortcutKey,
    UiAction,
};
use crate::infrastructure::{
    ConfigRepository, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_LEFT, BTN_RIGHT, BTN_SQUARE,
    BTN_TRIANGLE, BTN_UP,
};

/// Face buttons that can be declared as an element's shortcut.
const SHORTCUT_BUTTONS: [(u32, ShortcutKey); 3] = [
    (BTN_CIRCLE, ShortcutKey::Circle),
    (BTN_SQUARE, ShortcutKey::Square),
    (BTN_TRIANGLE, ShortcutKey::Triangle),
];

pub struct SharesController;

impl SharesController {
    /// Element under a touch tap at (`x`, `y`), if any.
    pub fn tap(state: &AppState, x: usize, y: usize) -> Option<UiAction> {
        state.hit_map.action_at(x as i32, y as i32)
    }

    /// Moves the D-pad focus (`+ New Share` header, then one row per share with its buttons)
    /// and returns the focused element's action when `Cross` is pressed.
    pub fn dpad(state: &mut AppState, buttons: u32) -> Option<UiAction> {
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
        if buttons & BTN_LEFT != 0 {
            if state.nav.col == 0 || state.nav.header {
                state.nav.focus = NavFocus::Sidebar;
            } else {
                state.nav.col -= 1;
            }
        }
        if buttons & BTN_RIGHT != 0
            && !state.nav.header
            && state.nav.col + 1 < action_count(ActiveTab::Shares)
        {
            state.nav.col += 1;
        }
        if buttons & BTN_CROSS == 0 {
            return None;
        }
        state.hit_map.focused_action(Self::focus_slot(state))
    }

    /// Element triggered directly by a face button of `pressed`, if any.
    pub fn shortcut(state: &AppState, pressed: u32) -> Option<UiAction> {
        SHORTCUT_BUTTONS
            .iter()
            .filter(|(button, _)| pressed & button != 0)
            .find_map(|(_, key)| {
                state
                    .hit_map
                    .shortcut_action(*key, state.selected_share_idx)
            })
    }

    /// D-pad focus position on the page.
    pub fn focus_slot(state: &AppState) -> FocusSlot {
        if state.nav.header {
            FocusSlot::Header
        } else {
            FocusSlot::RowAction {
                row: state.selected_share_idx,
                col: state.nav.col,
            }
        }
    }

    /// Executes `action`. A share index that is no longer valid (the list changed since the
    /// frame was drawn) is ignored.
    pub fn apply(state: &mut AppState, repo: &ConfigRepository, action: UiAction) {
        let share_idx = match action {
            UiAction::NewShare => {
                state.modal = ModalDialog::CreateShare { selected_idx: 0 };
                return;
            }
            UiAction::SelectShare(idx)
            | UiAction::ManageDevices(idx)
            | UiAction::DeleteShare(idx)
            | UiAction::ChangeShareFolder(idx) => idx,
        };
        if share_idx >= state.shares.len() {
            return;
        }
        state.selected_share_idx = share_idx;
        match action {
            UiAction::ManageDevices(_) => {
                state.modal = ModalDialog::ManageDevices {
                    share_idx,
                    selected_dev_idx: 0,
                };
            }
            UiAction::DeleteShare(_) => {
                state.modal = ModalDialog::ConfirmDeleteShare { share_idx };
            }
            UiAction::ChangeShareFolder(_) => {
                state.picker_mode = PickerMode::ChangeShareTarget;
                ShareService::refresh_picker_items(state, repo);
                state.active_tab = ActiveTab::FolderPicker;
            }
            UiAction::NewShare | UiAction::SelectShare(_) => {}
        }
    }
}
