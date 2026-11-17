//! D-pad navigation model: tab order, focusable buttons per row and entry positions.

use vita_syncthing_app::domain::{
    action_count, default_col, settings_row_action_count, starts_on_header, ActiveTab, NavFocus,
    NavState, DASHBOARD_HEADER_ACTIONS, SETTINGS_ROWS, SETTINGS_ROW_DECLINED, SETTINGS_ROW_DEVICES,
    SETTINGS_ROW_DISPLAY, SETTINGS_ROW_LANGUAGE, SETTINGS_ROW_LOG, SETTINGS_ROW_PLUGIN, TAB_ORDER,
};

#[test]
fn test_sidebar_tab_order() {
    assert_eq!(
        TAB_ORDER,
        [
            ActiveTab::Dashboard,
            ActiveTab::FolderPicker,
            ActiveTab::Settings
        ]
    );
}

#[test]
fn test_settings_card_indices_are_consecutive_and_counted() {
    assert_eq!(SETTINGS_ROW_DISPLAY, 0);
    assert_eq!(SETTINGS_ROW_DEVICES, 1);
    assert_eq!(SETTINGS_ROW_LANGUAGE, 2);
    assert_eq!(SETTINGS_ROW_DECLINED, 3);
    assert_eq!(SETTINGS_ROW_PLUGIN, 4);
    assert_eq!(SETTINGS_ROW_LOG, 5);
    assert_eq!(SETTINGS_ROWS, 6);
    assert_eq!(SETTINGS_ROW_LOG + 1, SETTINGS_ROWS);
}

#[test]
fn test_overview_device_card_has_two_header_buttons() {
    assert_eq!(DASHBOARD_HEADER_ACTIONS, 2);
}

#[test]
fn test_action_count_per_tab() {
    assert_eq!(
        action_count(ActiveTab::Dashboard),
        3,
        "Devices, Delete, Folder"
    );
    assert_eq!(
        action_count(ActiveTab::FolderPicker),
        0,
        "Files rows have no buttons"
    );
    assert_eq!(action_count(ActiveTab::Settings), 1);
}

#[test]
fn test_settings_row_action_count_devices_and_plugin_cards_have_two_buttons() {
    assert_eq!(settings_row_action_count(SETTINGS_ROW_DISPLAY), 1);
    assert_eq!(settings_row_action_count(SETTINGS_ROW_DEVICES), 2);
    assert_eq!(settings_row_action_count(SETTINGS_ROW_LANGUAGE), 1);
    assert_eq!(settings_row_action_count(SETTINGS_ROW_DECLINED), 1);
    assert_eq!(settings_row_action_count(SETTINGS_ROW_PLUGIN), 2);
    assert_eq!(settings_row_action_count(SETTINGS_ROW_LOG), 1);
    // Unknown rows have the single default button.
    assert_eq!(settings_row_action_count(SETTINGS_ROWS), 1);
}

#[test]
fn test_entry_positions_per_tab() {
    assert!(starts_on_header(ActiveTab::Dashboard));
    assert!(!starts_on_header(ActiveTab::FolderPicker));
    assert!(!starts_on_header(ActiveTab::Settings));
    for tab in TAB_ORDER {
        assert_eq!(
            default_col(tab),
            0,
            "entering a tab focuses its first button"
        );
    }
}

#[test]
fn test_new_nav_starts_on_the_sidebar_at_the_tab_entry() {
    let dash = NavState::new(ActiveTab::Dashboard);
    assert_eq!(dash.focus, NavFocus::Sidebar);
    assert_eq!((dash.col, dash.header, dash.row), (0, true, 0));

    let files = NavState::new(ActiveTab::FolderPicker);
    assert_eq!(files.focus, NavFocus::Sidebar);
    assert!(!files.header);

    let settings = NavState::new(ActiveTab::Settings);
    assert_eq!(settings.focus, NavFocus::Sidebar);
    assert!(!settings.header);
    assert_eq!(settings.row, 0);
}

#[test]
fn test_nav_focus_variants_differ() {
    assert_ne!(NavFocus::Sidebar, NavFocus::Content);
}
