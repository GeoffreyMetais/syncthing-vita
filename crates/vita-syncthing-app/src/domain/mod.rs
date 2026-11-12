//! Domain Layer: Pure business entities, value objects, and application state.
//! Zero dependencies on filesystem I/O, network sockets, PS Vita FFI, or 2D rendering.

pub mod app_state;
pub mod defaults;
pub mod initial_sync;
pub mod interaction;
pub mod models;
pub mod navigation;
pub mod ui_text;

pub use app_state::AppState;
pub use initial_sync::InitialSyncPeers;
pub use interaction::{FocusSlot, HitMap, Hotspot, PadKey, Rect, ShortcutKey, UiAction};
pub use ui_text::UiText;
pub use models::{
    fmt_size, ActiveTab, CameraPreview, FolderOffer, IgnoredFolder, KnownDevice, Language,
    LanguagePreference, ModalDialog, PickerItem, PickerMode, ShareConfigEntry, ThemeMode, ADD_DEVICE_KEYPAD,
    BROWSER_ROWS, NEW_SHARE_TEMPLATES, VISIBLE_DEVICES,
};
pub use navigation::{
    action_count, default_col, settings_row_action_count, NavFocus, NavState, SETTINGS_ROWS,
    SETTINGS_ROW_DECLINED, SETTINGS_ROW_DEVICES, SETTINGS_ROW_DISPLAY, SETTINGS_ROW_LANGUAGE,
    TAB_ORDER,
};
