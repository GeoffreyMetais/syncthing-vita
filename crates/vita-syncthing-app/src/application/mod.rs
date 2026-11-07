//! Application Layer: Use cases (`ShareService`), background worker event coordinator
//! (`SyncCoordinator`), and user input presenter/controller (`InputController`).

pub mod input_controller;
pub mod share_service;
pub mod sync_coordinator;

pub use input_controller::{InputController, UserCommand};
pub use share_service::ShareService;
pub use sync_coordinator::SyncCoordinator;
