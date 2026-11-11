//! Domain Layer: Pure business entities, value objects, and application state.
//! Zero dependencies on filesystem I/O, network sockets, PS Vita FFI, or 2D rendering.

pub mod app_state;
pub mod models;
pub mod navigation;

pub use app_state::AppState;
pub use models::{
    fmt_size, ActiveTab, CameraPreview, FolderOffer, IgnoredFolder, KnownDevice, Language,
    ModalDialog, PickerItem, PickerMode, ShareConfigEntry, ThemeMode, ADD_DEVICE_KEYPAD,
    BROWSER_ROWS, NEW_SHARE_TEMPLATES,
};
pub use navigation::{action_count, default_col, NavFocus, NavState, SETTINGS_ROWS, TAB_ORDER};
