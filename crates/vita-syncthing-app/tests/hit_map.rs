//! `HitMap`: touch, focus and face-button resolution of the elements declared by a frame.

use vita_syncthing_app::domain::{
    ActiveTab, FocusSlot, HitMap, Hotspot, PadKey, Rect, ScrollArea, ScrollTarget, ShortcutKey,
    UiAction,
};

fn spot(
    rect: Rect,
    action: UiAction,
    focus: Option<FocusSlot>,
    shortcut: Option<ShortcutKey>,
) -> Hotspot {
    Hotspot {
        rect,
        action,
        focus,
        shortcut,
    }
}

const BOX: Rect = Rect::new(10, 20, 100, 50);

#[test]
fn test_rect_contains_includes_top_left_and_excludes_bottom_right() {
    assert!(BOX.contains(10, 20));
    assert!(BOX.contains(109, 69));
    assert!(!BOX.contains(110, 20));
    assert!(!BOX.contains(10, 70));
    assert!(!BOX.contains(9, 20));
    assert!(!BOX.contains(10, 19));
}

#[test]
fn test_rect_new_stores_the_fields() {
    let r = Rect::new(1, 2, 3, 4);
    assert_eq!((r.x, r.y, r.w, r.h), (1, 2, 3, 4));
}

#[test]
fn test_action_at_returns_the_element_under_the_touch() {
    let mut map = HitMap::default();
    map.add(spot(BOX, UiAction::StartSync, None, None));
    assert_eq!(map.action_at(10, 20), Some(UiAction::StartSync));
    assert_eq!(map.action_at(109, 69), Some(UiAction::StartSync));
    assert_eq!(map.action_at(110, 69), None);
    assert_eq!(map.action_at(0, 0), None);
}

#[test]
fn test_action_at_prefers_the_topmost_overlapping_element() {
    // A button drawn on top of a card: the card is declared first.
    let mut map = HitMap::default();
    map.add(spot(BOX, UiAction::OpenLogs, None, None));
    map.add(spot(
        Rect::new(50, 30, 20, 10),
        UiAction::CloseLogs,
        None,
        None,
    ));
    assert_eq!(map.action_at(55, 35), Some(UiAction::CloseLogs));
    assert_eq!(map.action_at(12, 22), Some(UiAction::OpenLogs));
}

#[test]
fn test_clear_forgets_hotspots_and_scroll_areas() {
    let mut map = HitMap::default();
    map.add(spot(BOX, UiAction::StartSync, None, None));
    map.add_scroll_area(ScrollArea {
        rect: BOX,
        target: ScrollTarget::FilesList,
        step: 10,
    });
    map.clear();
    assert!(map.hotspots().is_empty());
    assert!(map.scroll_areas().is_empty());
    assert_eq!(map.action_at(10, 20), None);
    assert_eq!(map.scroll_area_at(10, 20), None);
}

#[test]
fn test_hotspots_keep_drawing_order() {
    let mut map = HitMap::default();
    map.add(spot(BOX, UiAction::StartSync, None, None));
    map.add(spot(BOX, UiAction::ShowDeviceId, None, None));
    let actions: Vec<UiAction> = map.hotspots().iter().map(|h| h.action).collect();
    assert_eq!(actions, vec![UiAction::StartSync, UiAction::ShowDeviceId]);
}

#[test]
fn test_scroll_area_at_returns_topmost_area_and_respects_bounds() {
    let mut map = HitMap::default();
    assert_eq!(map.scroll_area_at(15, 25), None);
    map.add_scroll_area(ScrollArea {
        rect: BOX,
        target: ScrollTarget::FilesList,
        step: 12,
    });
    map.add_scroll_area(ScrollArea {
        rect: Rect::new(0, 0, 40, 40),
        target: ScrollTarget::FilesList,
        step: 3,
    });
    // Inside both: the last declared one wins.
    assert_eq!(map.scroll_area_at(15, 25).map(|a| a.step), Some(3));
    // Only inside the first.
    assert_eq!(map.scroll_area_at(50, 40).map(|a| a.step), Some(12));
    assert_eq!(map.scroll_area_at(110, 40), None);
    assert_eq!(map.scroll_areas().len(), 2);
}

#[test]
fn test_focused_action_matches_the_exact_slot() {
    let mut map = HitMap::default();
    let slot = FocusSlot::RowAction { row: 1, col: 2 };
    map.add(spot(BOX, UiAction::DeleteShare(1), Some(slot), None));
    map.add(spot(
        BOX,
        UiAction::ManageDevices(1),
        Some(FocusSlot::RowAction { row: 1, col: 1 }),
        None,
    ));
    map.add(spot(
        BOX,
        UiAction::NewShare,
        Some(FocusSlot::Header { col: 0 }),
        None,
    ));
    assert_eq!(map.focused_action(slot), Some(UiAction::DeleteShare(1)));
    assert_eq!(
        map.focused_action(FocusSlot::RowAction { row: 1, col: 1 }),
        Some(UiAction::ManageDevices(1))
    );
    assert_eq!(
        map.focused_action(FocusSlot::Header { col: 0 }),
        Some(UiAction::NewShare)
    );
    assert_eq!(
        map.focused_action(FocusSlot::Header { col: 1 }),
        None,
        "no element holds header column 1"
    );
    assert_eq!(
        map.focused_action(FocusSlot::RowAction { row: 2, col: 2 }),
        None
    );
}

#[test]
fn test_focused_action_ignores_elements_without_focus() {
    let mut map = HitMap::default();
    map.add(spot(BOX, UiAction::Inert, None, None));
    assert_eq!(map.focused_action(FocusSlot::Header { col: 0 }), None);
    assert_eq!(
        map.focused_action(FocusSlot::RowAction { row: 0, col: 0 }),
        None
    );
}

#[test]
fn test_focus_slot_row_is_none_for_the_header() {
    assert_eq!(FocusSlot::Header { col: 3 }.row(), None);
    assert_eq!(FocusSlot::RowAction { row: 4, col: 0 }.row(), Some(4));
    assert_eq!(FocusSlot::RowAction { row: 0, col: 9 }.row(), Some(0));
}

#[test]
fn test_shortcut_prefers_the_selected_row_over_page_elements() {
    let mut map = HitMap::default();
    let page = spot(BOX, UiAction::NewShare, None, Some(ShortcutKey::Triangle));
    let row0 = spot(
        BOX,
        UiAction::ChangeShareFolder(0),
        Some(FocusSlot::RowAction { row: 0, col: 2 }),
        Some(ShortcutKey::Triangle),
    );
    let row1 = spot(
        BOX,
        UiAction::ChangeShareFolder(1),
        Some(FocusSlot::RowAction { row: 1, col: 2 }),
        Some(ShortcutKey::Triangle),
    );
    map.add(page);
    map.add(row0);
    map.add(row1);

    assert_eq!(
        map.shortcut_action(ShortcutKey::Triangle, Some(1)),
        Some(UiAction::ChangeShareFolder(1))
    );
    assert_eq!(
        map.shortcut_action(ShortcutKey::Triangle, Some(0)),
        Some(UiAction::ChangeShareFolder(0))
    );
    // No selected row: the page-level element answers.
    assert_eq!(
        map.shortcut_action(ShortcutKey::Triangle, None),
        Some(UiAction::NewShare)
    );
    // A selected row without a Triangle element falls back to the page-level element.
    assert_eq!(
        map.shortcut_action(ShortcutKey::Triangle, Some(7)),
        Some(UiAction::NewShare)
    );
}

#[test]
fn test_shortcut_matches_the_key_only() {
    let mut map = HitMap::default();
    map.add(spot(
        BOX,
        UiAction::NewShare,
        None,
        Some(ShortcutKey::Triangle),
    ));
    map.add(spot(
        BOX,
        UiAction::DeleteShare(0),
        None,
        Some(ShortcutKey::Square),
    ));
    assert_eq!(
        map.shortcut_action(ShortcutKey::Square, None),
        Some(UiAction::DeleteShare(0))
    );
    assert_eq!(
        map.shortcut_action(ShortcutKey::Triangle, None),
        Some(UiAction::NewShare)
    );
    assert_eq!(map.shortcut_action(ShortcutKey::Circle, None), None);
}

#[test]
fn test_shortcut_never_answers_from_another_row() {
    let mut map = HitMap::default();
    map.add(spot(
        BOX,
        UiAction::DeleteShare(2),
        Some(FocusSlot::RowAction { row: 2, col: 1 }),
        Some(ShortcutKey::Square),
    ));
    assert_eq!(map.shortcut_action(ShortcutKey::Square, Some(0)), None);
    assert_eq!(map.shortcut_action(ShortcutKey::Square, None), None);
    assert_eq!(
        map.shortcut_action(ShortcutKey::Square, Some(2)),
        Some(UiAction::DeleteShare(2))
    );
}

#[test]
fn test_shortcut_of_a_header_element_is_page_level() {
    let mut map = HitMap::default();
    map.add(spot(
        BOX,
        UiAction::ShowDeviceId,
        Some(FocusSlot::Header { col: 0 }),
        Some(ShortcutKey::Circle),
    ));
    // Header elements have no row, so they answer whatever the selected row.
    assert_eq!(
        map.shortcut_action(ShortcutKey::Circle, Some(3)),
        Some(UiAction::ShowDeviceId)
    );
    assert_eq!(
        map.shortcut_action(ShortcutKey::Circle, None),
        Some(UiAction::ShowDeviceId)
    );
}

#[test]
fn test_pad_key_and_tab_actions_are_plain_values() {
    // Actions compare by value: a touch on a `Press` element is the pad button it stands for.
    assert_eq!(
        UiAction::Press(PadKey::Start),
        UiAction::Press(PadKey::Start)
    );
    assert_ne!(
        UiAction::Press(PadKey::Start),
        UiAction::Press(PadKey::Select)
    );
    assert_ne!(
        UiAction::SwitchTab(ActiveTab::Dashboard),
        UiAction::SwitchTab(ActiveTab::Settings)
    );
    assert_ne!(UiAction::ModalRow(0), UiAction::ModalRow(1));
}
