//! Domain Layer: Pure business entities, value objects, and application state.
//! Zero dependencies on filesystem I/O, network sockets, PS Vita FFI, or 2D rendering.

pub mod app_state;
pub mod models;
pub mod navigation;
pub mod ui_text;

pub use app_state::AppState;
pub use ui_text::UiText;
pub use models::{
    fmt_size, ActiveTab, CameraPreview, FolderOffer, IgnoredFolder, KnownDevice, Language,
    LanguagePreference, ModalDialog, PickerItem, PickerMode, ShareConfigEntry, ThemeMode, ADD_DEVICE_KEYPAD,
    BROWSER_ROWS, NEW_SHARE_TEMPLATES,
};
pub use navigation::{
    action_count, default_col, NavFocus, NavState, SETTINGS_ROWS, SETTINGS_ROW_DECLINED,
    SETTINGS_ROW_DISPLAY, SETTINGS_ROW_LANGUAGE, TAB_ORDER,
};
