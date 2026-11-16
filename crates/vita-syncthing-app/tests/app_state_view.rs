//! `AppState` view-model queries: D-pad focus slots, toasts, language selection, protocol log
//! buffer, manual sync preparation and the device summary of a share.

use syncthing_core::DeviceId;
use vita_syncthing_app::domain::defaults::LOG_BUFFER_LINES;
use vita_syncthing_app::domain::{
    ActiveTab, AppState, FocusSlot, KnownDevice, Language, LanguagePreference, LogView,
    ModalDialog, NavFocus, ShareConfigEntry, ShareMode, UiText, SETTINGS_ROW_DEVICES,
    SETTINGS_ROW_LOG,
};

const A: DeviceId = DeviceId([0x41; 32]);
const B: DeviceId = DeviceId([0x42; 32]);

fn share(folder_id: &str, linked: &[DeviceId]) -> ShareConfigEntry {
    ShareConfigEntry {
        folder_id: folder_id.to_owned(),
        label: folder_id.to_owned(),
        target_path: "ux0:data".to_owned(),
        enabled: true,
        mode: ShareMode::ReadWrite,
        shared_devices: linked.to_vec(),
    }
}

fn state_with_shares(n: usize) -> AppState {
    let shares = (0..n).map(|i| share(&format!("s{i}"), &[])).collect();
    AppState::new(shares, Vec::new(), "ux0:")
}

fn focus_content(s: &mut AppState, tab: ActiveTab) {
    s.active_tab = tab;
    s.nav.focus = NavFocus::Content;
}

#[test]
fn test_reset_nav_for_tab_restores_the_entry_position_and_keeps_focus_side() {
    let mut s = state_with_shares(2);
    s.active_tab = ActiveTab::Settings;
    s.nav.focus = NavFocus::Content;
    s.nav.row = 3;
    s.nav.col = 2;
    s.nav.header = true;

    s.reset_nav_for_tab();
    assert_eq!((s.nav.row, s.nav.col, s.nav.header), (0, 0, false));
    assert_eq!(s.nav.focus, NavFocus::Content, "focus side is kept");

    s.active_tab = ActiveTab::Dashboard;
    s.nav.col = 1;
    s.reset_nav_for_tab();
    assert!(s.nav.header, "Overview starts on its device card");
    assert_eq!(s.nav.col, 0);
}

#[test]
fn test_focus_slot_overview_header_clamps_the_column() {
    let mut s = state_with_shares(2);
    s.nav.header = true;
    s.nav.col = 0;
    assert_eq!(s.focus_slot(), FocusSlot::Header { col: 0 });
    s.nav.col = 1;
    assert_eq!(s.focus_slot(), FocusSlot::Header { col: 1 });
    s.nav.col = 7;
    assert_eq!(s.focus_slot(), FocusSlot::Header { col: 1 });
}

#[test]
fn test_focus_slot_overview_without_shares_is_always_the_header() {
    let mut s = state_with_shares(0);
    s.nav.header = false;
    s.nav.col = 1;
    assert_eq!(s.focus_slot(), FocusSlot::Header { col: 1 });
}

#[test]
fn test_focus_slot_overview_row_uses_the_selected_share() {
    let mut s = state_with_shares(3);
    s.nav.header = false;
    s.selected_share_idx = 2;
    s.nav.col = 1;
    assert_eq!(s.focus_slot(), FocusSlot::RowAction { row: 2, col: 1 });
}

#[test]
fn test_focus_slot_files_is_header_or_selected_row_column_zero() {
    let mut s = state_with_shares(0);
    s.active_tab = ActiveTab::FolderPicker;
    s.nav.header = true;
    s.nav.col = 2;
    assert_eq!(s.focus_slot(), FocusSlot::Header { col: 0 });

    s.nav.header = false;
    s.picker.selected_idx = 4;
    s.nav.col = 3;
    assert_eq!(s.focus_slot(), FocusSlot::RowAction { row: 4, col: 0 });
}

#[test]
fn test_focus_slot_settings_clamps_column_to_the_card_buttons() {
    let mut s = state_with_shares(0);
    s.active_tab = ActiveTab::Settings;
    s.nav.row = SETTINGS_ROW_DEVICES; // two buttons: QR / ID and + Add Device
    s.nav.col = 1;
    assert_eq!(s.focus_slot(), FocusSlot::RowAction { row: 1, col: 1 });
    s.nav.col = 5;
    assert_eq!(s.focus_slot(), FocusSlot::RowAction { row: 1, col: 1 });

    s.nav.row = SETTINGS_ROW_LOG; // one button: Open
    s.nav.col = 3;
    assert_eq!(
        s.focus_slot(),
        FocusSlot::RowAction {
            row: SETTINGS_ROW_LOG,
            col: 0
        }
    );
}

#[test]
fn test_is_focused_requires_page_focus_and_the_matching_slot() {
    let mut s = state_with_shares(1);
    s.nav.header = true;
    s.nav.col = 0;
    let header = FocusSlot::Header { col: 0 };
    s.active_tab = ActiveTab::Dashboard;
    assert!(!s.is_focused(header), "sidebar has the focus");
    s.nav.focus = NavFocus::Content;
    assert!(s.is_focused(header));
    assert!(!s.is_focused(FocusSlot::Header { col: 1 }));
    assert!(!s.is_focused(FocusSlot::RowAction { row: 0, col: 0 }));
}

#[test]
fn test_focused_action_is_the_column_of_a_row_on_the_page() {
    let mut s = state_with_shares(2);
    focus_content(&mut s, ActiveTab::Dashboard);
    s.nav.header = false;
    s.nav.col = 2;
    assert_eq!(s.focused_action(ActiveTab::Dashboard), Some(2));
    assert_eq!(s.focused_action(ActiveTab::Settings), None, "other tab");

    s.nav.header = true;
    assert_eq!(
        s.focused_action(ActiveTab::Dashboard),
        None,
        "header is not a row"
    );

    s.nav.header = false;
    s.nav.focus = NavFocus::Sidebar;
    assert_eq!(
        s.focused_action(ActiveTab::Dashboard),
        None,
        "sidebar has the focus"
    );
}

#[test]
fn test_is_share_row_focused_only_for_an_existing_share_row() {
    let mut s = state_with_shares(2);
    focus_content(&mut s, ActiveTab::Dashboard);
    s.nav.header = false;
    s.selected_share_idx = 1;
    assert!(s.is_share_row_focused());
    s.selected_share_idx = 2;
    assert!(!s.is_share_row_focused(), "index past the last share");
    s.selected_share_idx = 0;
    s.nav.header = true;
    assert!(!s.is_share_row_focused(), "device card has the focus");
    s.nav.header = false;
    s.active_tab = ActiveTab::Settings;
    assert!(!s.is_share_row_focused(), "not on Overview");
}

#[test]
fn test_is_settings_row_focused_checks_the_card_row() {
    let mut s = state_with_shares(0);
    focus_content(&mut s, ActiveTab::Settings);
    s.nav.row = 2;
    assert!(s.is_settings_row_focused(2));
    assert!(!s.is_settings_row_focused(1));
    s.nav.focus = NavFocus::Sidebar;
    assert!(!s.is_settings_row_focused(2));
    s.nav.focus = NavFocus::Content;
    s.active_tab = ActiveTab::Dashboard;
    assert!(!s.is_settings_row_focused(2), "other tab");
}

#[test]
fn test_is_header_focused_checks_tab_focus_and_header_flag() {
    let mut s = state_with_shares(1);
    focus_content(&mut s, ActiveTab::Dashboard);
    s.nav.header = true;
    assert!(s.is_header_focused(ActiveTab::Dashboard));
    assert!(!s.is_header_focused(ActiveTab::FolderPicker));
    s.nav.header = false;
    assert!(!s.is_header_focused(ActiveTab::Dashboard));
    s.nav.header = true;
    s.nav.focus = NavFocus::Sidebar;
    assert!(!s.is_header_focused(ActiveTab::Dashboard));
}

#[test]
fn test_toast_lasts_110_vblanks_then_expires_once() {
    let mut s = state_with_shares(0);
    assert!(!s.tick_toast(), "no toast");
    s.show_toast(UiText::SyncingWithPeers);
    assert_eq!(s.toast_msg, Some((UiText::SyncingWithPeers, 110)));
    for _ in 0..109 {
        assert!(!s.tick_toast());
    }
    assert_eq!(s.toast_msg, Some((UiText::SyncingWithPeers, 1)));
    assert!(s.tick_toast(), "the 110th VBlank expires it");
    assert!(s.toast_msg.is_none());
    assert!(!s.tick_toast());
}

#[test]
fn test_progress_animation_only_advances_while_indeterminate() {
    let mut s = state_with_shares(0);
    assert_eq!(s.progress_pct, Some(0));
    s.tick_progress_anim();
    assert_eq!(s.progress_anim_tick, 0);

    s.progress_pct = None;
    s.tick_progress_anim();
    s.tick_progress_anim();
    assert_eq!(s.progress_anim_tick, 2);

    s.progress_anim_tick = u32::MAX;
    s.tick_progress_anim();
    assert_eq!(s.progress_anim_tick, 0, "wraps around");
}

#[test]
fn test_set_language_preference_resolves_against_the_system_language() {
    let mut s = state_with_shares(0);
    s.set_system_language(Language::German);
    assert_eq!(s.language, Language::German);
    s.set_language_preference(LanguagePreference::Fixed(Language::French));
    assert_eq!(s.language, Language::French);
    s.set_system_language(Language::Spanish);
    assert_eq!(
        s.language,
        Language::French,
        "fixed language ignores the system"
    );
    assert_eq!(s.system_language, Language::Spanish);
    s.set_language_preference(LanguagePreference::System);
    assert_eq!(s.language, Language::Spanish);
}

#[test]
fn test_cycle_language_walks_all_choices_and_announces_each() {
    let mut s = state_with_shares(0);
    s.set_system_language(Language::Italian);
    s.cycle_language();
    assert_eq!(
        s.language_pref,
        LanguagePreference::Fixed(Language::English)
    );
    assert_eq!(s.language, Language::English);
    assert_eq!(
        s.toast_msg,
        Some((UiText::LanguageChanged(Language::English), 110))
    );

    s.cycle_language();
    assert_eq!(s.language, Language::French);
    s.cycle_language();
    assert_eq!(s.language, Language::Spanish);
    s.cycle_language();
    assert_eq!(s.language, Language::German);
    s.cycle_language();
    assert_eq!(s.language, Language::Italian);

    s.cycle_language();
    assert_eq!(s.language_pref, LanguagePreference::System);
    assert_eq!(s.language, Language::Italian, "back to the system language");
    assert_eq!(
        s.toast_msg,
        Some((UiText::LanguageChanged(Language::Italian), 110))
    );
}

#[test]
fn test_push_log_sanitizes_and_keeps_the_newest_lines() {
    let mut s = state_with_shares(0);
    s.push_log("\u{1b}[0m first ");
    assert_eq!(s.logs, vec!["[0m first".to_owned()]);

    for i in 0..LOG_BUFFER_LINES {
        s.push_log(&format!("line {i}"));
    }
    assert_eq!(s.logs.len(), LOG_BUFFER_LINES);
    assert_eq!(
        s.logs.last().unwrap(),
        &format!("line {}", LOG_BUFFER_LINES - 1)
    );
    assert_eq!(
        s.logs.first().unwrap(),
        "line 0",
        "the first line was just dropped? no: 201 in total"
    );
}

#[test]
fn test_push_log_feeds_an_open_viewer() {
    let mut s = state_with_shares(0);
    s.log_view = Some(LogView {
        from_end: 3,
        ..LogView::new()
    });
    s.push_log("new");
    assert_eq!(
        s.log_view.as_ref().unwrap().from_end,
        4,
        "scrolled-back window kept"
    );

    s.log_view = Some(LogView {
        scope: vita_syncthing_app::domain::LogScope::History,
        history: vec!["old".to_owned()],
        from_end: 0,
    });
    s.push_log("fresh");
    assert_eq!(s.log_view.as_ref().unwrap().history, vec!["old", "fresh"]);
}

#[test]
fn test_prepare_manual_sync_waits_for_a_peer_when_one_is_configured() {
    let mut s = AppState::new(
        Vec::new(),
        vec![KnownDevice {
            device_id: A,
            name: "a".to_owned(),
            address: String::new(),
        }],
        "ux0:",
    );
    s.files_downloaded = 4;
    s.bytes_transferred = 99;
    s.prepare_manual_sync();
    assert!(!s.sync_in_progress, "a configured peer must connect first");
    assert_eq!(s.files_downloaded, 0);
    assert_eq!(s.bytes_transferred, 0);
    assert_eq!(s.progress_pct, None);
    assert_eq!(s.status_line, UiText::StartingSync);
    assert_eq!(s.toast_msg, Some((UiText::SyncingWithPeers, 110)));

    s.session_connected = true;
    s.prepare_manual_sync();
    assert!(s.sync_in_progress, "a live session will emit a sync cycle");
}

#[test]
fn test_prepare_manual_sync_without_peers_starts_a_download_phase() {
    let mut s = AppState::new(Vec::new(), Vec::new(), "ux0:");
    s.prepare_manual_sync();
    assert!(s.sync_in_progress, "a LAN scan will emit Finished");
    assert_eq!(s.blocks_uploaded, 0);
    assert_eq!(s.files_up_to_date, 0);
}

#[test]
fn test_share_devices_summary_lists_known_and_unknown_devices() {
    let mut s = state_with_shares(0);
    s.devices = vec![KnownDevice {
        device_id: A,
        name: "Phone".to_owned(),
        address: String::new(),
    }];
    let none = share("x", &[]);
    assert_eq!(s.share_devices_summary(&none), UiText::NoDevicesLinked);

    let linked = share("x", &[A, B]);
    let expected = format!("Phone ({}), {}", A.short(), B.short());
    assert_eq!(
        s.share_devices_summary(&linked),
        UiText::SharedWith { devices: expected }
    );
}

#[test]
fn test_new_state_starts_with_no_modal_and_no_viewer() {
    let s = state_with_shares(0);
    assert_eq!(s.modal, ModalDialog::None);
    assert_eq!(s.toast_msg, None);
    assert!(s.log_view.is_none());
    assert_eq!(s.selected_share_idx, 0);
    assert_eq!(s.drag_scrolled_rows, 0);
}
