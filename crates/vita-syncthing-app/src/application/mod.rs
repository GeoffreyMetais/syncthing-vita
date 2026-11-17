//! Application Layer: Use cases (`ShareService`, `ShareCreation`, `FolderOfferService`), QR code pairing (`QrScanFlow`), background worker event coordinator
//! (`SyncCoordinator`), and user input presenter/controller (`InputController`).

pub mod controllers;
pub mod device_registry;
pub mod devices_scan;
pub mod dpad_navigation;
pub mod folder_browser;
pub mod folder_offers;
pub mod frame_pacing;
pub mod input_gestures;
pub mod modal_input;
pub mod plugin_service;
pub mod power_policy;
pub mod qr_scan;
pub mod share_creation;
pub mod share_mode;
pub mod share_service;
pub mod sync_coordinator;
pub mod sync_supervisor;
pub mod upload_activity;

pub use controllers::{
    DashboardController, FilesController, InputController, LogsController, SettingsController,
    SharesController, UserCommand,
};
pub use device_registry::DeviceRegistry;
pub use devices_scan::DevicesScan;
pub use dpad_navigation::DpadNavigation;
pub use folder_browser::FolderBrowser;
pub use folder_offers::FolderOfferService;
pub use frame_pacing::{FramePacer, FrameSignals};
pub use input_gestures::InputGestures;
pub use modal_input::ModalInput;
pub use plugin_service::{PluginError, PluginService};
pub use qr_scan::{extract_device_id, QrScanFlow};
pub use share_creation::ShareCreation;
pub use share_mode::ShareModeService;
pub use share_service::ShareService;
pub use sync_coordinator::SyncCoordinator;
pub use sync_supervisor::SyncSupervisor;
pub use upload_activity::UploadActivity;
