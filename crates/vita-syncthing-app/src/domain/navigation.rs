//! D-pad focus model: the focus is either on the sidebar tabs or on the current page.
//! Page rows reuse `AppState::selected_share_idx` / `selected_picker_idx`; `col` is the focused
//! action button within a row and `header` the page's top button (`+ New Share`, `Up`).

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
    /// True when the page's top button is focused instead of a row.
    pub header: bool,
    /// Focused card on pages with fixed rows (Settings: `SETTINGS_ROW_*`).
    pub row: usize,
}

impl NavState {
    pub fn new(tab: ActiveTab) -> Self {
        Self {
            focus: NavFocus::Sidebar,
            col: default_col(tab),
            header: false,
            row: 0,
        }
    }
}

/// Sidebar order of the tabs.
pub const TAB_ORDER: [ActiveTab; 4] = [
    ActiveTab::Dashboard,
    ActiveTab::Shares,
    ActiveTab::FolderPicker,
    ActiveTab::Settings,
];

/// Number of focusable action buttons per row, left to right:
/// Dashboard `[QR / ID, Sync Now]`, Shares `[Devices, Delete, Folder]`, Settings
/// `[+ Add Device, QR / ID, Switch Theme]`. Files rows have no buttons (the row is the target).
pub const fn action_count(tab: ActiveTab) -> usize {
    match tab {
        ActiveTab::Dashboard => 2,
        ActiveTab::Shares => 3,
        ActiveTab::FolderPicker => 0,
        ActiveTab::Settings => 3,
    }
}

/// Settings card with the display buttons (`+ Add Device`, `QR / ID`, `Switch Theme`).
pub const SETTINGS_ROW_DISPLAY: usize = 0;
/// Settings card with the `Change Language` button.
pub const SETTINGS_ROW_LANGUAGE: usize = 1;
/// Settings card with the `Clear Declined` button.
pub const SETTINGS_ROW_DECLINED: usize = 2;
/// Number of focusable cards on the Settings page.
pub const SETTINGS_ROWS: usize = 3;

/// Button focused when entering a tab: the one `Cross` triggered before D-pad navigation.
pub const fn default_col(tab: ActiveTab) -> usize {
    match tab {
        ActiveTab::Dashboard => 0,
        ActiveTab::Settings => 2,
        ActiveTab::Shares | ActiveTab::FolderPicker => 0,
    }
}
