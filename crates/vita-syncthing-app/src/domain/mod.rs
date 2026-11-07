//! Domain Layer: Pure business entities, value objects, and application state.
//! Zero dependencies on filesystem I/O, network sockets, PS Vita FFI, or 2D rendering.

pub mod app_state;
pub mod models;

pub use app_state::AppState;
pub use models::{
    fmt_size, ActiveTab, KnownDevice, ModalDialog, PickerItem, PickerMode, ShareConfigEntry,
    ThemeMode, BROWSER_ROWS, NEW_SHARE_TEMPLATES,
};
