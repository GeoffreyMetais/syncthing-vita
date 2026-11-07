//! Feature-based screen views for `vita-syncthing-app`:
//! - `dashboard`: Tab 1 (`Overview` — device hero card, QR/ID & Sync actions, activity feed)
//! - `shares`: Tab 2 (`Shared Folders` — share cards, `+ New Share`, `Devices`, `Pause`, `Folder`)
//! - `folder_picker`: Tab 3 (`Files` — interactive `ux0:` directory browser & presets)
//! - `settings`: Tab 4 (`Settings & Log` — Dark/OLED theme switcher & BEP/1.0 protocol log)
//! - `screensaver`: Bouncing DVD-style OLED screensaver (`#000000` background)

pub mod dashboard;
pub mod folder_picker;
pub mod screensaver;
pub mod settings;
pub mod shares;

pub use dashboard::render_dashboard_tab;
pub use folder_picker::render_folder_picker_tab;
pub use screensaver::{render_oled_screensaver, SAVER_H, SAVER_W};
pub use settings::render_settings_tab;
pub use shares::render_shares_tab;
