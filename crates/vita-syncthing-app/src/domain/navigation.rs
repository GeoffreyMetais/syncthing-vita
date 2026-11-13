//! D-pad focus model: the focus is either on the sidebar tabs or on the current page.
//! Page rows reuse `AppState::selected_share_idx` / `selected_picker_idx`; `col` is the focused
//! action button within a row and `header` the page's top element (`Overview`: the device card
//! buttons, `Files`: the `Up` button).

use crate::domain::models::ActiveTab;

/// Which part of the screen receives D-pad input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavFocus {
    Sidebar,
    Content,
}

/// D-pad focus position on the main screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavState {
    pub focus: NavFocus,
    /// Focused action button (left to right) in the current row.
    pub col: usize,
    /// True when the page's top element is focused instead of a row.
    pub header: bool,
    /// Focused card on pages with fixed rows (Settings: `SETTINGS_ROW_*`).
    pub row: usize,
}

impl NavState {
    pub fn new(tab: ActiveTab) -> Self {
        Self {
            focus: NavFocus::Sidebar,
            col: default_col(tab),
            header: starts_on_header(tab),
            row: 0,
        }
    }
}

/// Sidebar order of the tabs.
pub const TAB_ORDER: [ActiveTab; 3] = [
    ActiveTab::Dashboard,
    ActiveTab::FolderPicker,
    ActiveTab::Settings,
];

/// Number of focusable action buttons per row, left to right:
/// Overview share rows `[Devices, Delete, Folder]`, Settings `[Switch Theme]` on Display
/// (row 0) and `[QR / ID, + Add Device]` on Devices (row 1).
/// Files rows have no buttons (the row is the target).
pub const fn action_count(tab: ActiveTab) -> usize {
    match tab {
        ActiveTab::Dashboard => 3,
        ActiveTab::FolderPicker => 0,
        ActiveTab::Settings => 1,
    }
}

/// Number of focusable action buttons on each Settings card.
pub const fn settings_row_action_count(row: usize) -> usize {
    match row {
        SETTINGS_ROW_DEVICES => 2,
        _ => 1,
    }
}

/// Settings card with the display button (`Switch Theme`).
pub const SETTINGS_ROW_DISPLAY: usize = 0;
/// Settings card with the device pairing buttons (`+ Add Device`, `QR / ID`).
pub const SETTINGS_ROW_DEVICES: usize = 1;
/// Settings card with the `Change Language` button.
pub const SETTINGS_ROW_LANGUAGE: usize = 2;
/// Settings card with the `Clear Declined` button.
pub const SETTINGS_ROW_DECLINED: usize = 3;
/// Number of focusable cards on the Settings page.
pub const SETTINGS_ROWS: usize = 4;

/// Focusable buttons of the `Overview` device card (`[QR / ID, New Share]`), its header row.
pub const DASHBOARD_HEADER_ACTIONS: usize = 2;

/// Button focused when entering a tab: the first one of the row.
pub const fn default_col(_tab: ActiveTab) -> usize {
    0
}

/// True when entering `tab` focuses its top element: the device card buttons of `Overview`.
pub const fn starts_on_header(tab: ActiveTab) -> bool {
    matches!(tab, ActiveTab::Dashboard)
}
