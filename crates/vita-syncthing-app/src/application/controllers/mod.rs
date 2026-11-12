//! Input & Page Controllers: `InputController` routes main-screen gamepad and touch input
//! against the frame's `HitMap`, and each page controller (`DashboardController`,
//! `SharesController`, `FilesController`, `SettingsController`) handles D-pad focus movement
//! and page-specific `UiAction`s.

pub mod dashboard;
pub mod files;
pub mod input;
pub mod settings;
pub mod shares;

pub use dashboard::DashboardController;
pub use files::FilesController;
pub use input::{InputController, UserCommand};
pub use settings::SettingsController;
pub use shares::SharesController;
