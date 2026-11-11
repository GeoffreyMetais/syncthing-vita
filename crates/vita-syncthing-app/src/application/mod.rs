//! Application Layer: Use cases (`ShareService`, `ShareCreation`), QR code pairing (`QrScanFlow`), background worker event coordinator
//! (`SyncCoordinator`), and user input presenter/controller (`InputController`).

pub mod dpad_navigation;
pub mod input_controller;
pub mod modal_input;
pub mod power_policy;
pub mod qr_scan;
pub mod share_creation;
pub mod share_service;
pub mod sync_coordinator;

pub use dpad_navigation::{DpadNavigation, PageAction};
pub use input_controller::{InputController, UserCommand};
pub use modal_input::ModalInput;
pub use qr_scan::{extract_device_id, QrScanFlow};
pub use share_creation::ShareCreation;
pub use share_service::ShareService;
pub use sync_coordinator::SyncCoordinator;
