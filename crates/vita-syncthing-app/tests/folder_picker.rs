//! `Files` tab (`ux0:` browser): `Circle` walks up the device-root path, then leaves the tab.

mod common;

use common::{press, temp_test_dir};
use vita_syncthing_app::domain::{ActiveTab, AppState};
use vita_syncthing_app::infrastructure::{ConfigRepository, BTN_CIRCLE};

#[test]
fn test_files_tab_up_navigation_walks_device_root_paths() {
    let dir = temp_test_dir("picker-up");
    let repo = ConfigRepository::new(&dir);
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:data/retroarch");
    state.active_tab = ActiveTab::FolderPicker;
    state.selected_picker_idx = 3;

    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.picker_current_dir, "ux0:data");
    assert_eq!(state.selected_picker_idx, 0);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.picker_current_dir, "ux0:");
    assert_eq!(state.active_tab, ActiveTab::FolderPicker);
    press(&mut state, &repo, BTN_CIRCLE);
    assert_eq!(state.active_tab, ActiveTab::Dashboard);

    let _ = std::fs::remove_dir_all(&dir);
}
