//! Application Layer: Use cases (`ShareService`, `ShareCreation`, `FolderOfferService`), QR code pairing (`QrScanFlow`), background worker event coordinator
//! (`SyncCoordinator`), and user input presenter/controller (`InputController`).

pub mod controllers;
pub mod devices_scan;
pub mod dpad_navigation;
pub mod folder_offers;
pub mod frame_pacing;
pub mod modal_input;
pub mod power_policy;
pub mod qr_scan;
pub mod share_creation;
pub mod share_mode;
pub mod share_service;
pub mod sync_coordinator;
pub mod upload_activity;

pub use controllers::{
    DashboardController, FilesController, InputController, SettingsController, SharesController,
    UserCommand,
};
pub use devices_scan::DevicesScan;
pub use dpad_navigation::DpadNavigation;
pub use folder_offers::FolderOfferService;
pub use frame_pacing::{FramePacer, FrameSignals};
pub use modal_input::ModalInput;
pub use qr_scan::{extract_device_id, QrScanFlow};
pub use share_creation::ShareCreation;
pub use share_mode::ShareModeService;
pub use share_service::ShareService;
pub use sync_coordinator::SyncCoordinator;
pub use upload_activity::UploadActivity;
