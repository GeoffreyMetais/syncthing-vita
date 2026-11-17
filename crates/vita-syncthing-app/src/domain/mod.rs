//! Domain Layer: Pure business entities, value objects, and application state.
//! Zero dependencies on filesystem I/O, network sockets, PS Vita FFI, or 2D rendering.

pub mod app_state;
pub mod defaults;
pub mod folder_picker;
pub mod initial_sync;
pub mod interaction;
pub mod log_view;
pub mod models;
pub mod navigation;
pub mod plugin;
pub mod plugin_config;
pub mod screensaver;
pub mod tai_config;
pub mod ui_text;

pub use app_state::AppState;
pub use folder_picker::FolderPicker;
pub use initial_sync::{ConnectedPeers, InitialSyncPeers, PeerSet};
pub use interaction::{
    FocusSlot, HitMap, Hotspot, PadKey, Rect, ScrollArea, ScrollTarget, ShortcutKey, UiAction,
};
pub use log_view::{sanitize_log_line, LogScope, LogView, LOG_VIEW_PAGE, LOG_VIEW_ROWS};
pub use models::{
    fmt_size, ActiveTab, CameraPreview, FolderOffer, IgnoredFolder, KnownDevice, Language,
    LanguagePreference, ModalDialog, PickerItem, PickerMode, ShareConfigEntry, ShareMode,
    SyncActivity, ThemeMode, ADD_DEVICE_KEYPAD, BROWSER_ROWS, BROWSE_TEMPLATE_ID,
    NEW_SHARE_TEMPLATES, VISIBLE_DEVICES,
};
pub use navigation::{
    action_count, default_col, settings_row_action_count, starts_on_header, NavFocus, NavState,
    DASHBOARD_HEADER_ACTIONS, SETTINGS_ROWS, SETTINGS_ROW_DECLINED, SETTINGS_ROW_DEVICES,
    SETTINGS_ROW_DISPLAY, SETTINGS_ROW_LANGUAGE, SETTINGS_ROW_LOG, SETTINGS_ROW_PLUGIN, TAB_ORDER,
};
pub use plugin::{PluginInfo, PluginNotice, PluginRequest, PluginStatus};
pub use plugin_config::PluginConfig;
pub use screensaver::ScreensaverMotion;
pub use ui_text::UiText;
