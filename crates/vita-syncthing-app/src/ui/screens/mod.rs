//! Feature-based screen views for `vita-syncthing-app`:
//! - `dashboard`: Tab 1 (`Overview` — device card, QR/ID & Sync actions, share list)
//! - `shares`: share list of the `Overview` (share cards with `Devices`, `Delete`, `Folder`)
//! - `folder_picker`: Tab 2 (`Files` — interactive `ux0:` directory browser & presets)
//! - `settings`: Tab 3 (`Settings & Log` — Dark/OLED theme switcher & BEP/1.0 protocol log)
//! - `logs`: full-screen protocol log viewer opened from `Settings` (session lines / `Show All`)
//! - `screensaver`: Bouncing DVD-style OLED screensaver (`#000000` background)

pub mod dashboard;
pub mod folder_picker;
pub mod logs;
pub mod screensaver;
pub mod settings;
pub mod shares;

pub use dashboard::render_dashboard_tab;
pub use folder_picker::render_folder_picker_tab;
pub use logs::render_log_screen;
pub use screensaver::{render_oled_screensaver, SAVER_H, SAVER_W};
pub use settings::render_settings_tab;
